/* Generated from schema/api.schema.json by `npm run gen-types`. Do not edit. */

export type ConversationKind = ("dm" | "group_dm") | "self" | "channel";
export type Presence = "online" | "away" | "dnd" | "offline";
export type Role = "owner" | "admin" | "member";
/**
 * Who can find and join a channel. Group DMs, DMs and notes are always `private`.
 */
export type ChannelVisibility = "public" | "private";
export type FormFieldKind = "text" | "multiline" | "boolean" | "select" | "multi_select";
export type MessageKind = "text" | "file" | "system";
/**
 * Attachment storage backend configuration. Only config, accounts and the text database live on the server's disk when a cloud backend is selected.
 */
export type StorageConfig =
  | {
      backend: "local";
      path?: string | null;
    }
  | {
      access_key_id: string;
      allow_http?: boolean;
      backend: "s3";
      bucket: string;
      endpoint?: string | null;
      path_style?: boolean;
      prefix?: string | null;
      provider: S3Provider;
      region: string;
      secret_access_key: string;
    }
  | {
      /**
       * Override for tests / proxies. Default `https://www.googleapis.com`.
       */
      api_base?: string | null;
      backend: "gdrive";
      client_id: string;
      client_secret: string;
      folder_name?: string | null;
      refresh_token: string;
      token_url?: string | null;
    };
export type S3Provider = "aws" | "r2" | "b2" | "minio" | "other";
/**
 * Who may create channels.
 */
export type ChannelCreation = "everyone" | "admins";
/**
 * Who may pin messages in channels (DMs and group DMs: always every member).
 */
export type ChannelPins = "everyone" | "managers";
/**
 * App Home block (A-04).
 */
export type HomeBlock =
  | {
      text: string;
      type: "header";
    }
  | {
      text: string;
      type: "text";
    }
  | {
      type: "divider";
    }
  | {
      id: string;
      label: string;
      style?: string | null;
      type: "button";
    }
  | {
      form: Form;
      type: "form";
    };
/**
 * Messages from client to server. The first message must be `hello`.
 */
export type ClientWsMsg =
  | {
      client_version?: string;
      cursor: number;
      t: "hello";
      token: string;
    }
  | {
      t: "ping";
    }
  | {
      conv_id: string;
      t: "typing";
    }
  | {
      idle: boolean;
      t: "idle";
    }
  | {
      cursor: number;
      t: "ack";
    }
  | {
      conv_id: string;
      payload: unknown;
      t: "call_signal";
    };
/**
 * Stable, machine-readable error codes. New codes may be added; existing ones never change meaning.
 */
export type ErrorCode =
  | (
      | "invalid_credentials"
      | "rate_limited"
      | "forbidden"
      | "not_found"
      | "validation"
      | "conflict"
      | "file_too_large"
      | "setup_done"
      | "storage_error"
      | "not_implemented"
      | "internal"
    )
  | "unauthorized"
  | "token_expired"
  | "session_revoked"
  | "account_disabled"
  | "password_reset"
  | "history_cleared"
  | "locked_out"
  | "edit_window_closed"
  | "cursor_expired"
  | "client_too_old"
  | "setup_required"
  | "backup_failed";
export type EventData =
  | {
      message: Message;
      type: "message_new";
    }
  | {
      message: Message;
      type: "message_edit";
    }
  | {
      message: Message;
      type: "message_delete";
    }
  | {
      conv_id: string;
      message_ids: string[];
      type: "message_hide";
    }
  | {
      conv_id: string;
      message_id: string;
      reactions: ReactionSummary[];
      type: "reaction_update";
    }
  | {
      conv_id: string;
      message_id: string;
      type: "message_delivered";
    }
  | {
      conversation: ConversationSummary;
      type: "conversation_upsert";
    }
  | {
      conv_id: string;
      type: "conversation_remove";
    }
  | {
      conv_id: string;
      last_read_seq: number;
      type: "read_receipt";
      user_id: string;
    }
  | {
      conv_id: string;
      last_read_seq: number;
      marked_unread: boolean;
      mentions: number;
      type: "read_update";
      unread: number;
    }
  | {
      type: "user_upsert";
      user: User;
    }
  | {
      attachment_id: string;
      type: "attachment_removed";
    }
  | {
      me: Me;
      type: "me_update";
    }
  | {
      thread: ThreadSummary;
      type: "thread_update";
    }
  | {
      conv_id: string;
      pins: Pin[];
      type: "pins_update";
    }
  | {
      bookmarks: Bookmark[];
      conv_id: string;
      type: "bookmarks_update";
    }
  | {
      groups: UserGroup[];
      type: "groups_update";
    }
  | {
      pref: ConvPref;
      type: "pref_update";
    }
  | {
      sections: Section[];
      type: "sections_update";
    }
  | {
      attachment?: string | null;
      kind: string;
      target: string;
      type: "avatar_override_update";
    }
  | {
      draft: DraftItem;
      type: "draft_update";
    }
  | {
      item: ScheduledMessage;
      type: "scheduled_update";
    }
  | {
      item: SavedItem;
      type: "saved_update";
    }
  | {
      message_id: string;
      type: "saved_remove";
    }
  | {
      labels: Label[];
      type: "labels_update";
    }
  | {
      read_ms: number;
      type: "activity_read";
    }
  | {
      settings: OrgSettings;
      type: "org_settings_update";
    }
  | {
      channel?: ChannelBrowseItem | null;
      conv_id: string;
      type: "channel_directory_update";
    }
  | {
      type: "resync";
    };
export type ScheduledState = "pending" | "sent" | "cancelled" | "failed";
export type ServerWsMsg =
  | {
      cursor: number;
      server_time: number;
      t: "hello_ok";
    }
  | {
      caught_up: boolean;
      events: Event[];
      t: "events";
    }
  | {
      conv_id: string;
      t: "typing";
      user_id: string;
    }
  | {
      last_active_ms?: number | null;
      presence: Presence;
      t: "presence";
      user_id: string;
    }
  | {
      t: "pong";
    }
  | {
      code: ErrorCode;
      message: string;
      t: "error";
    };

/**
 * Every public type, used only to produce one JSON Schema document with all definitions.
 */
export interface WorkChatAPI {
  admin_bot: AdminBot;
  admin_conversation: AdminConversation;
  admin_create_user: CreateUserRequest;
  admin_create_user_response: CreateUserResponse;
  admin_delete_messages: AdminDeleteMessagesRequest;
  admin_delete_messages_response: AdminDeleteMessagesResponse;
  admin_set_role: SetRoleRequest;
  admin_update_user: UpdateUserRequest;
  admin_updates: AdminUpdates;
  ai_defaults_update: AiDefaultsUpdate;
  ai_overview: AiOverview;
  ai_provider: AiProvider;
  ai_provider_input: AiProviderInput;
  ai_provider_update: AiProviderUpdate;
  ai_recap_request: AiRecapRequest;
  ai_recap_response: AiRecapResponse;
  ai_search_request: AiSearchRequest;
  ai_search_response: AiSearchResponse;
  ai_test: AiTestResponse;
  avatar_override: AvatarOverride;
  backup_settings: BackupSettings;
  backup_settings_update: BackupSettingsUpdate;
  backups: BackupInfo[];
  bookmark: Bookmark;
  bookmark_request: BookmarkRequest;
  bootstrap: Bootstrap;
  bot_home: BotHome;
  bot_profile: BotProfile;
  channel_browse_item: ChannelBrowseItem;
  channel_browse_page: ChannelBrowsePage;
  clear_history: ClearHistoryResponse;
  client_ws: ClientWsMsg;
  conv_pref: ConvPref;
  conv_pref_request: ConvPrefRequest;
  counts: Counts;
  create_bot: CreateBotRequest;
  create_conversation: CreateConversation;
  create_incoming_webhook: CreateIncomingWebhook;
  create_outgoing_webhook: CreateOutgoingWebhook;
  delete_range: DateRangeDeleteRequest;
  draft_item: DraftItem;
  draft_request: DraftRequest;
  edit_message: EditMessage;
  error: ErrorBody;
  event: Event;
  events: EventsResponse;
  file_hit: FileHit;
  file_hits: FileHit[];
  form: Form;
  form_submit: FormSubmit;
  forward_request: ForwardRequest;
  fresh_restore_request: FreshRestoreRequest;
  fresh_restore_response: FreshRestoreResponse;
  hide_message_response: HideMessageResponse;
  history: HistoryResponse;
  job: JobStatus;
  link_login_request: LinkLoginRequest;
  login_request: LoginRequest;
  login_response: LoginResponse;
  me: Me;
  members_request: MembersRequest;
  meta: ServerMeta;
  org_settings: OrgSettings;
  pin: Pin;
  public_url_settings: PublicUrlSettings;
  public_url_update: PublicUrlUpdate;
  push_registration: PushRegistration;
  push_settings: PushSettings;
  refresh_request: RefreshRequest;
  release_info: ReleaseInfo;
  restore_request: RestoreRequest;
  restore_response: RestoreResponse;
  schedule_request: ScheduleRequest;
  scheduled: ScheduledMessage;
  search: SearchResponse;
  section: Section;
  section_request: SectionRequest;
  send_message: SendMessage;
  server_ws: ServerWsMsg;
  sessions: SessionInfo[];
  set_conv_avatar: SetConvAvatar;
  setup: SetupRequest;
  storage_config: StorageConfig;
  storage_test: StorageTestResponse;
  thread_summaries: ThreadSummary[];
  thread_summary: ThreadSummary;
  thread_view: ThreadView;
  token_pair: TokenPair;
  update_bot: UpdateBotRequest;
  update_conversation: UpdateConversation;
  update_org_settings: UpdateOrgSettings;
  update_outgoing_webhook: UpdateOutgoingWebhook;
  upload: UploadResponse;
  user_group: UserGroup;
  user_group_request: UserGroupRequest;
  viewer_conversation: ViewerConversation;
  viewer_history: ViewerHistory;
  webhooks: WebhooksOverview;
}
export interface AdminBot {
  conversations: BotConvRef[];
  created_ms: number;
  description: string;
  /**
   * Number of updates waiting for `getUpdates`.
   */
  queued_updates: number;
  /**
   * Full bearer token (admins only). Use it as `/bot<token>/<method>` or `Authorization: Bearer <token>`.
   */
  token: string;
  user: User;
  webhook?: BotWebhookInfo | null;
}
export interface BotConvRef {
  id: string;
  kind: ConversationKind;
  name: string;
}
export interface User {
  /**
   * Attachment id of the profile photo; clients show initials when absent.
   */
  avatar?: string | null;
  disabled?: boolean;
  display_name: string;
  id: string;
  is_bot?: boolean;
  /**
   * Never present when the user has chosen to hide it (SV-12).
   */
  last_active_ms?: number | null;
  /**
   * Work phone number as typed by the person or the admin; free text.
   */
  phone?: string | null;
  presence?: Presence | null;
  role: Role;
  /**
   * Job title shown on the profile and in the contacts list ("Head of Product Operations").
   */
  title?: string | null;
  username: string;
}
export interface BotWebhookInfo {
  has_secret: boolean;
  last_error?: string | null;
  last_error_ms?: number | null;
  pending_updates: number;
  url: string;
}
export interface AdminConversation {
  archived?: boolean;
  /**
   * The picture of a channel or group DM that everyone sees (an attachment id with purpose `avatar`).
   */
  avatar?: string | null;
  created_ms: number;
  id: string;
  kind: ConversationKind;
  last_message_ts?: number | null;
  /**
   * Highest message sequence number in this conversation.
   */
  last_seq: number;
  members: Member[];
  message_count: number;
  name?: string | null;
  topic?: string | null;
  /**
   * `public` or `private` (channels only; everything else is `private`). Channels that existed before 1.1.0 are `private`.
   */
  visibility?: ChannelVisibility & string;
}
export interface Member {
  /**
   * Read receipts: how far this member has read (conversation sequence number). Only set in `dm` and `group_dm` conversations and only when the member has read receipts switched on. A message of yours is "read" in a DM when the other member's `last_read_seq` >= its `seq`. Kept current by the `read_receipt` event.
   */
  last_read_seq?: number | null;
  /**
   * "owner" or "member" (conversation-local role). The conversation `owner` is the channel manager: they can rename, set topic and picture, add and remove members, archive and (always) pin. The creator of a channel or group DM gets it.
   */
  role: string;
  user_id: string;
}
export interface CreateUserRequest {
  /**
   * Also create a one-time login link (default true when no password is given).
   */
  create_login_link?: boolean | null;
  display_name: string;
  email?: string | null;
  /**
   * Leave empty to create the account with a random password and a one-time login link only.
   */
  password?: string | null;
  phone?: string | null;
  role?: Role | null;
  title?: string | null;
  username: string;
}
export interface CreateUserResponse {
  login_link?: LoginLink | null;
  user: AdminUser;
}
export interface LoginLink {
  expires_ms: number;
  /**
   * Paste this whole link (or just the token) on the app's login page.
   */
  link: string;
  token: string;
}
export interface AdminUser {
  /**
   * Attachment id of the profile photo; clients show initials when absent.
   */
  avatar?: string | null;
  created_ms: number;
  deleted: boolean;
  disabled?: boolean;
  display_name: string;
  email?: string | null;
  hide_last_active: boolean;
  id: string;
  is_bot?: boolean;
  /**
   * Never present when the user has chosen to hide it (SV-12).
   */
  last_active_ms?: number | null;
  /**
   * Always the real value for admins, regardless of `hide_last_active`.
   */
  last_active_real_ms?: number | null;
  password_set: boolean;
  /**
   * Work phone number as typed by the person or the admin; free text.
   */
  phone?: string | null;
  presence?: Presence | null;
  privacy_controls_enabled: boolean;
  role: Role;
  session_count: number;
  /**
   * Job title shown on the profile and in the contacts list ("Head of Product Operations").
   */
  title?: string | null;
  username: string;
}
export interface AdminDeleteMessagesRequest {
  /**
   * Delete everything strictly before this time (in `conv_id`).
   */
  before_ms?: number | null;
  conv_id?: string | null;
  from_ms?: number | null;
  /**
   * Specific messages; or leave empty and use `conv_id` plus a date range.
   */
  message_ids?: string[];
  /**
   * "everyone" (tombstone for all) or "user" (only `user_id`'s copy disappears).
   */
  scope: string;
  to_ms?: number | null;
  user_id?: string | null;
}
export interface AdminDeleteMessagesResponse {
  attachments_moved: number;
  backup_id: string;
  messages_deleted: number;
}
/**
 * Body of `PUT /admin/users/{id}/role` (owner only).
 */
export interface SetRoleRequest {
  role: Role;
}
export interface UpdateUserRequest {
  disabled?: boolean | null;
  display_name?: string | null;
  email?: string | null;
  phone?: string | null;
  /**
   * Turns the Privacy screen on for this user (SV-12).
   */
  privacy_controls_enabled?: boolean | null;
  role?: Role | null;
  title?: string | null;
}
/**
 * The admin panel's "Updates" card: this server's version against the newest GitHub release.
 */
export interface AdminUpdates {
  /**
   * When the server last asked GitHub (ms since the epoch).
   */
  checked_ms: number;
  /**
   * This server's version.
   */
  current: string;
  /**
   * Why the check failed (offline, rate limit...), shown instead of the release.
   */
  error?: string | null;
  /**
   * The newest published release (drafts and pre-releases skipped), even when it is not newer.
   */
  latest?: ReleaseInfo | null;
  /**
   * The address that was checked.
   */
  source: string;
  /**
   * `latest` is newer than `current`.
   */
  update_available: boolean;
}
export interface ReleaseInfo {
  assets: Asset[];
  html_url: string;
  name: string;
  /**
   * Release notes as plain text, trimmed.
   */
  notes: string;
  published: string;
  /**
   * The tag as published (`v1.3.1`).
   */
  tag: string;
  /**
   * Version without the `v` (`1.3.1`).
   */
  version: string;
}
export interface Asset {
  name: string;
  size: number;
  url: string;
}
/**
 * Which provider serves which feature. An empty string clears the choice (= automatic).
 */
export interface AiDefaultsUpdate {
  recap_provider?: string | null;
  search_provider?: string | null;
}
/**
 * All providers plus which one serves which feature. `None` = the first enabled provider with a key.
 */
export interface AiOverview {
  /**
   * Shown by the admin panel: message text is sent to the provider.
   */
  notice: string;
  providers: AiProvider[];
  recap_provider?: string | null;
  search_provider?: string | null;
}
/**
 * One configured AI provider as the admin panel sees it. The API key is never returned.
 */
export interface AiProvider {
  /**
   * Overrides the provider's default API address (OpenAI-compatible servers, proxies).
   */
  base_url?: string;
  enabled: boolean;
  has_key: boolean;
  id: string;
  /**
   * Last four characters of the key (only when the key is long enough for that to be safe).
   */
  key_hint?: string;
  /**
   * "anthropic" or "openai" (any OpenAI-compatible endpoint).
   */
  kind: string;
  model: string;
  name: string;
}
export interface AiProviderInput {
  api_key: string;
  base_url?: string | null;
  enabled?: boolean | null;
  /**
   * "anthropic" or "openai" ("openai-compatible" is accepted as an alias).
   */
  kind: string;
  model?: string | null;
  name: string;
}
export interface AiProviderUpdate {
  /**
   * A new key; `None` or an empty string keeps the stored one.
   */
  api_key?: string | null;
  base_url?: string | null;
  enabled?: boolean | null;
  kind?: string | null;
  model?: string | null;
  name?: string | null;
}
export interface AiRecapRequest {
  conv_id: string;
  /**
   * Only messages after this time (UTC ms); default the last 24 hours.
   */
  since_ms?: number | null;
}
export interface AiRecapResponse {
  message_count: number;
  summary: string;
}
export interface AiSearchRequest {
  /**
   * Also ask the model for a short answer based on the top hits.
   */
  answer?: boolean;
  query: string;
}
export interface AiSearchResponse {
  answer?: string | null;
  interpreted: AiInterpretation;
  results: SearchResponse;
}
export interface AiInterpretation {
  after_ms?: number | null;
  before_ms?: number | null;
  from?: string | null;
  has?: string | null;
  in_conv?: string | null;
  q: string;
}
export interface SearchResponse {
  /**
   * Channels and group DMs of the caller whose name matches (SR-05).
   */
  channels?: ConversationSummary[];
  /**
   * Attachments whose file name matches (SR-03).
   */
  files?: FileHit[];
  hits: SearchHit[];
  people: User[];
}
/**
 * A conversation plus the calling user's own state in it.
 */
export interface ConversationSummary {
  archived?: boolean;
  /**
   * The picture of a channel or group DM that everyone sees (an attachment id with purpose `avatar`).
   */
  avatar?: string | null;
  created_ms: number;
  id: string;
  kind: ConversationKind;
  last_message_ts?: number | null;
  last_read_seq: number;
  /**
   * Highest message sequence number in this conversation.
   */
  last_seq: number;
  marked_unread: boolean;
  members: Member[];
  mentions: number;
  name?: string | null;
  topic?: string | null;
  unread: number;
  /**
   * `public` or `private` (channels only; everything else is `private`). Channels that existed before 1.1.0 are `private`.
   */
  visibility?: ChannelVisibility & string;
}
/**
 * A file in the Files browser / file search (F-05, SR-03).
 */
export interface FileHit {
  attachment: Attachment;
  conv_id: string;
  message_id: string;
  sender_id: string;
  ts: number;
}
export interface Attachment {
  content_type: string;
  id: string;
  /**
   * M-30/M-31: duration, waveform peaks, size and poster frame of voice messages and videos (set by the uploader).
   */
  meta?: AttachmentMeta | null;
  name: string;
  /**
   * True when an admin removed the file; clients show "File removed".
   */
  removed?: boolean;
  size: number;
}
/**
 * Media details the sender's app computes before sending: used to draw waveforms and video posters without downloading the file first. Everything is optional; the server only checks the size and field limits.
 */
export interface AttachmentMeta {
  duration_ms?: number | null;
  height?: number | null;
  /**
   * Page count of a PDF or slide deck ("4.2 MB - 28 pages" on document cards).
   */
  pages?: number | null;
  /**
   * Loudness per slice of the recording, 0..=255, 64 values at most.
   */
  peaks?: number[];
  /**
   * A small JPEG of the first video frame, base64 (at most about 40 KB).
   */
  poster_b64?: string | null;
  width?: number | null;
}
export interface SearchHit {
  message: Message;
  /**
   * Snippet with matches wrapped in `[[` and `]]`.
   */
  snippet: string;
}
export interface Message {
  /**
   * "Also send to channel": a thread reply (`thread_root` is set) that also shows in the conversation's main timeline. It is one message, not a copy: show it in the timeline as a reply to the thread root (`thread_root`), and in the thread as usual. Absent means false. Always false for messages that are not thread replies.
   */
  also_send_to_channel?: boolean;
  attachments?: Attachment[];
  /**
   * Plain text in the small markup (see docs/API.md). Empty for tombstones.
   */
  body: string;
  /**
   * Client-generated id for idempotent sends (only echoed to the sender's devices).
   */
  client_id?: string | null;
  conv_id: string;
  /**
   * Tombstone: shown as "Message deleted".
   */
  deleted?: boolean;
  /**
   * Delivered to at least one recipient device.
   */
  delivered?: boolean;
  edited_ms?: number | null;
  /**
   * SV-17: interactive form sent by a bot.
   */
  form?: Form | null;
  /**
   * M-20: set when this message was forwarded from another conversation (attribution).
   */
  forwarded?: ForwardInfo | null;
  id: string;
  kind: MessageKind;
  /**
   * Structured @mentions (user ids).
   */
  mentions?: string[];
  reactions?: ReactionSummary[];
  reply_to?: string | null;
  /**
   * Users who already submitted the form.
   */
  responded?: string[];
  sender_id: string;
  /**
   * Conversation-local, gapless, increasing.
   */
  seq: number;
  thread_root?: string | null;
  ts: number;
  /**
   * 1 for a never edited message.
   */
  version: number;
}
/**
 * A JSON form a bot attaches to a message. Rendered natively as a card (or a modal when a button carries its own form); the response goes back to the bot.
 */
export interface Form {
  buttons?: FormButton[];
  description?: string | null;
  fields?: FormField[];
  id: string;
  /**
   * Label of the submit button shown when the form has fields (default "Submit").
   */
  submit_label?: string | null;
  title?: string | null;
}
export interface FormButton {
  /**
   * A button can open a modal form (A-03); the response is sent with this button id.
   */
  form?: Form | null;
  id: string;
  label: string;
  /**
   * "primary", "danger" or "default".
   */
  style?: string | null;
}
export interface FormField {
  /**
   * String for text/select, bool for boolean, array of strings for multi_select.
   */
  default?: {
    [k: string]: unknown;
  };
  id: string;
  label?: string;
  options?: FormOption[];
  placeholder?: string | null;
  required?: boolean;
  type: FormFieldKind;
}
export interface FormOption {
  label?: string;
  value: string;
}
/**
 * Attribution of a forwarded message (M-20).
 */
export interface ForwardInfo {
  conv_id: string;
  message_id: string;
  sender_id: string;
  ts: number;
}
export interface ReactionSummary {
  emoji: string;
  user_ids: string[];
}
export interface AiTestResponse {
  message: string;
  ok: boolean;
}
/**
 * A private picture the caller chose for another user (`kind` = "user") or a conversation (`kind` = "conv"). Only the caller ever receives it; it wins over the person's or channel's own picture on the caller's devices.
 */
export interface AvatarOverride {
  /**
   * Attachment id (purpose `avatar_private`).
   */
  attachment: string;
  kind: string;
  target: string;
}
/**
 * Where and how often full backups run. The passphrase is never returned.
 */
export interface BackupSettings {
  enabled: boolean;
  /**
   * Hours between scheduled backups (24 = daily).
   */
  interval_hours: number;
  last_run_ms?: number | null;
  last_status: string;
  passphrase_set: boolean;
  /**
   * How many full backups to keep; older ones are deleted from the target.
   */
  retention: number;
  /**
   * Backup target. `None` means "the attachment storage".
   */
  target?: StorageConfig | null;
}
export interface BackupSettingsUpdate {
  /**
   * Use the attachment storage as the target again.
   */
  clear_target?: boolean;
  enabled?: boolean | null;
  interval_hours?: number | null;
  /**
   * Set or change the encryption passphrase (min. 8 characters).
   */
  passphrase?: string | null;
  retention?: number | null;
  target?: StorageConfig | null;
}
export interface BackupInfo {
  attachments: number;
  created_ms: number;
  /**
   * True when the backup file is encrypted with the backup passphrase (age).
   */
  encrypted?: boolean;
  id: string;
  key: string;
  /**
   * "user", "full" or "messages".
   */
  kind: string;
  reason: string;
  size: number;
  user_id?: string | null;
}
/**
 * A channel bookmark (C-07): a link or a file shortcut shown above the timeline.
 */
export interface Bookmark {
  attachment_id?: string | null;
  conv_id: string;
  created_by: string;
  id: string;
  position: number;
  title: string;
  url?: string | null;
}
export interface BookmarkRequest {
  attachment_id?: string | null;
  position?: number | null;
  title: string;
  url?: string | null;
}
export interface Bootstrap {
  /**
   * Activity items at or before this time are read.
   */
  activity_read_ms?: number;
  /**
   * The caller's private pictures for other users and conversations.
   */
  avatar_overrides?: AvatarOverride[];
  bookmarks?: {
    [k: string]: Bookmark[];
  };
  conversations: ConversationSummary[];
  /**
   * Start `events` / the WebSocket from this cursor after applying the snapshot.
   */
  cursor: number;
  /**
   * Synced drafts (M-22).
   */
  drafts?: DraftItem[];
  /**
   * User groups (`@design-team`), M-18.
   */
  groups?: UserGroup[];
  /**
   * Per conversation: true when older messages exist on the server.
   */
  has_more: {
    [k: string]: boolean;
  };
  labels?: Label[];
  me: Me;
  /**
   * Latest messages per conversation (conversation id -> ascending by seq).
   */
  messages: {
    [k: string]: Message[];
  };
  /**
   * The organisation settings (who can create channels, who can pin in channels).
   */
  org_settings?: OrgSettings;
  /**
   * Pins and bookmarks per conversation (SV-09).
   */
  pins?: {
    [k: string]: Pin[];
  };
  /**
   * Per-conversation favourites, sections, mute and notification level (NV-01..NV-07, N-03).
   */
  prefs?: ConvPref[];
  /**
   * Saved messages ("Later") and the user's labels.
   */
  saved?: SavedItem[];
  sections?: Section[];
  server_time: number;
  /**
   * Threads the user follows, newest activity first (M-06, M-07).
   */
  threads?: ThreadSummary[];
  users: User[];
}
/**
 * A draft, per conversation or per thread (M-22). An empty body means "no draft".
 */
export interface DraftItem {
  body: string;
  conv_id: string;
  thread_root?: string | null;
  updated_ms: number;
}
/**
 * A user group (`@handle`) that expands to member mentions when a message is sent (M-18).
 */
export interface UserGroup {
  handle: string;
  id: string;
  member_ids: string[];
  name: string;
}
/**
 * A label the user created for organising saved messages. Private to the user.
 */
export interface Label {
  /**
   * `#rrggbb`.
   */
  color: string;
  id: string;
  name: string;
  position: number;
}
export interface Me {
  email?: string | null;
  hide_last_active: boolean;
  /**
   * False for accounts created with a one-time link that have not set a password yet.
   */
  password_set: boolean;
  /**
   * What the user chose: "auto" (online while connected), "away" or "dnd".
   */
  presence_pref: string;
  /**
   * True when an admin enabled the Privacy screen for this user (SV-12).
   */
  privacy_controls_enabled: boolean;
  /**
   * Read receipts (default on): when off, other people do not see how far you have read in DMs and group chats.
   */
  read_receipts?: boolean;
  user: User;
}
/**
 * Organisation-wide settings, `GET /org-settings` (any signed-in user), changed by admins with `PUT /admin/org-settings`. Also part of the bootstrap and kept current by the `org_settings_update` event.
 */
export interface OrgSettings {
  /**
   * Who can create channels: `everyone` (default) or `admins`.
   */
  channel_creation?: ChannelCreation & string;
  /**
   * Who can pin in channels: `everyone` (default) or `managers`.
   */
  channel_pins?: ChannelPins & string;
}
/**
 * A pinned message (C-09). At most 100 per conversation.
 */
export interface Pin {
  conv_id: string;
  message_id: string;
  pinned_by: string;
  ts: number;
}
/**
 * Per-user state of one conversation (synced across devices).
 */
export interface ConvPref {
  conv_id: string;
  favourite?: boolean;
  /**
   * Muted until this time (UTC ms); `-1` means forever. Absent: not muted.
   */
  mute_until_ms?: number | null;
  /**
   * "all", "mentions" or "nothing" (N-03).
   */
  notify: string;
  /**
   * NV-04: the user's own order inside the section (lower first). Absent: not reordered, sorted by the list setting.
   */
  position?: number | null;
  section_id?: string | null;
}
/**
 * A message the user saved for later. The message itself is read from the normal message cache.
 */
export interface SavedItem {
  conv_id: string;
  label_ids?: string[];
  message_id: string;
  saved_ms: number;
}
/**
 * A custom sidebar section (NV-04).
 */
export interface Section {
  collapsed?: boolean;
  id: string;
  name: string;
  position: number;
}
/**
 * A thread the user follows (M-06).
 */
export interface ThreadSummary {
  conv_id: string;
  following: boolean;
  /**
   * The newest reply (for the thread preview line); absent while there are no replies.
   */
  last_reply?: Message | null;
  last_reply_ts: number;
  participants: string[];
  reply_count: number;
  root: Message;
  unread: number;
}
export interface BotHome {
  blocks?: HomeBlock[];
}
/**
 * What members see on a bot's profile (A-04).
 */
export interface BotProfile {
  description: string;
  home: BotHome;
  user: User;
}
/**
 * A public channel as listed by `GET /channels/browse`.
 */
export interface ChannelBrowseItem {
  /**
   * Attachment id of the channel picture.
   */
  avatar?: string | null;
  created_ms: number;
  id: string;
  member_count: number;
  name: string;
  topic?: string | null;
}
/**
 * `GET /channels/browse?q=&limit=&offset=`: public, not archived channels the caller is not in, by name.
 */
export interface ChannelBrowsePage {
  channels: ChannelBrowseItem[];
  has_more: boolean;
  /**
   * All matching channels, not just this page.
   */
  total: number;
}
export interface ClearHistoryResponse {
  attachments_moved: number;
  backup_id: string;
  backup_key: string;
  messages_hidden: number;
  sessions_revoked: number;
}
export interface ConvPrefRequest {
  favourite?: boolean | null;
  /**
   * `0` unmutes, `-1` mutes forever, otherwise mute until that time.
   */
  mute_until_ms?: number | null;
  notify?: string | null;
  /**
   * Empty string removes the conversation from its section.
   */
  section_id?: string | null;
}
/**
 * `GET /me/counts`: the numbers behind the filter pills (All / DMs / Channels / Threads / Files). Cheap to compute; clients refresh it on start and whenever a `thread_update`, `conversation_upsert` or `message_new` event arrives.
 */
export interface Counts {
  channels: number;
  /**
   * Conversations you are in, excluding archived ones and notes to self.
   */
  conversations: number;
  /**
   * Direct messages and group DMs.
   */
  dms: number;
  /**
   * Files in your conversations (not removed, not deleted messages).
   */
  files: number;
  files_audio: number;
  files_document: number;
  files_image: number;
  files_video: number;
  mentions: number;
  /**
   * Saved messages.
   */
  saved: number;
  /**
   * Threads you take part in (follow).
   */
  threads: number;
  /**
   * Threads with replies you have not read.
   */
  threads_unread: number;
  unread_channels: number;
  /**
   * Conversations with unread messages or marked unread.
   */
  unread_conversations: number;
  unread_dms: number;
  /**
   * Unread messages across all conversations (thread replies included).
   */
  unread_messages: number;
}
export interface CreateBotRequest {
  /**
   * Conversations (channels / group DMs) to add the bot to straight away.
   */
  conv_ids?: string[];
  description?: string;
  display_name: string;
  username: string;
}
/**
 * `POST /conversations`. Creating a `channel` is open to everyone unless the organisation setting `channel_creation` is `admins`; creating a `group_dm` is always open to everyone. The creator joins as a member with the conversation role `owner` (channel manager).
 */
export interface CreateConversation {
  /**
   * Admins only: set `false` to create the conversation without joining it (the admin panel does this). Default true.
   */
  include_creator?: boolean | null;
  /**
   * `channel` or `group_dm`.
   */
  kind: ConversationKind;
  /**
   * The other people to add (the creator is added automatically).
   */
  members?: string[];
  name: string;
  topic?: string | null;
  /**
   * Channels only: `public` (default) or `private`. A `group_dm` is always private.
   */
  visibility?: ChannelVisibility | null;
}
export interface CreateIncomingWebhook {
  conv_id: string;
  name: string;
}
export interface CreateOutgoingWebhook {
  conv_id?: string | null;
  events?: string[];
  name: string;
  secret?: string | null;
  url: string;
}
export interface DateRangeDeleteRequest {
  /**
   * Also hide the messages for every other member (default false: their copy stays).
   */
  also_other_copies?: boolean;
  /**
   * "Delete before date": every message strictly older than this time.
   */
  before_ms?: number | null;
  /**
   * Restrict to one conversation.
   */
  conversation_id?: string | null;
  /**
   * Start of the range (inclusive). Omit together with `before_ms` semantics for "everything before".
   */
  from_ms?: number;
  /**
   * End of the range (inclusive). Ignored when `before_ms` is set.
   */
  to_ms?: number;
}
export interface DraftRequest {
  body: string;
  thread_root?: string | null;
}
export interface EditMessage {
  body: string;
  mentions?: string[];
}
/**
 * Body of every non-2xx response.
 */
export interface ErrorBody {
  error: ApiError;
}
export interface ApiError {
  code: ErrorCode;
  message: string;
}
/**
 * One entry of a user's ordered event log. `cursor` increases monotonically per user.
 */
export interface Event {
  cursor: number;
  event: EventData;
  ts: number;
}
export interface ScheduledMessage {
  attachment_ids?: string[];
  body: string;
  conv_id: string;
  created_ms: number;
  due_ms: number;
  error?: string | null;
  id: string;
  message_id?: string | null;
  state: ScheduledState;
  thread_root?: string | null;
  user_id: string;
}
export interface EventsResponse {
  /**
   * Cursor to continue from (the last returned event, or the request cursor if none).
   */
  cursor: number;
  events: Event[];
  has_more: boolean;
}
export interface FormSubmit {
  /**
   * The pressed button; absent for the plain submit button of a form with fields.
   */
  button_id?: string | null;
  values?: {
    [k: string]: unknown;
  };
}
export interface ForwardRequest {
  /**
   * Optional note posted with the forwarded message.
   */
  comment?: string | null;
  conv_ids: string[];
}
/**
 * Restore of a full backup onto a fresh (empty) server.
 */
export interface FreshRestoreRequest {
  /**
   * Object key of the backup; default: the most recent one.
   */
  key?: string | null;
  passphrase: string;
  /**
   * Where the backups are kept; omit when `upload` is used.
   */
  target?: StorageConfig | null;
}
export interface FreshRestoreResponse {
  attachments: number;
  /**
   * Attachments found in the connected storage after the restore.
   */
  attachments_resolvable: number;
  conversations: number;
  messages: number;
  storage: string;
  users: number;
}
/**
 * `POST /messages/{id}/hide` ("Delete for me"): the message is hidden for the caller only, on all their devices.
 */
export interface HideMessageResponse {
  conv_id: string;
  message_id: string;
}
export interface HistoryResponse {
  has_more_after: boolean;
  has_more_before: boolean;
  messages: Message[];
}
export interface JobStatus {
  done: number;
  errors: string[];
  failed: number;
  finished_ms?: number | null;
  id: string;
  kind: string;
  message: string;
  started_ms: number;
  /**
   * "running", "done" or "failed".
   */
  state: string;
  total: number;
}
/**
 * Sign in with the one-time token from an admin-created login link.
 */
export interface LinkLoginRequest {
  client_version?: string;
  device_name?: string;
  token: string;
}
export interface LoginRequest {
  client_version?: string;
  device_name?: string;
  password: string;
  username: string;
}
export interface LoginResponse {
  access_token: string;
  /**
   * Seconds until `access_token` expires.
   */
  expires_in: number;
  me: Me;
  refresh_token: string;
  session_id: string;
}
export interface MembersRequest {
  user_ids: string[];
}
export interface ServerMeta {
  ai_search_enabled: boolean;
  api_version: number;
  fcm_enabled?: boolean;
  max_upload_bytes: number;
  min_client_version: string;
  name: string;
  ntfy_url?: string | null;
  server_version: string;
  /**
   * True until the first owner account has been created.
   */
  setup_required: boolean;
}
/**
 * The externally visible base URL used in login links (owner setting; the env var wins when set).
 */
export interface PublicUrlSettings {
  /**
   * What links are built from right now.
   */
  effective: string;
  /**
   * "env" (WORKCHAT_PUBLIC_URL), "setting" or "request" (derived from the Host header).
   */
  source: string;
  /**
   * The URL saved in the admin panel (empty when none).
   */
  url?: string | null;
}
export interface PublicUrlUpdate {
  /**
   * `null` or empty clears the setting.
   */
  url?: string | null;
}
/**
 * A push registration (SV-13). `kind` is "fcm", "ntfy", "unifiedpush" or "apns".
 */
export interface PushRegistration {
  created_ms?: number;
  device_name?: string;
  id?: string;
  kind: string;
  /**
   * ntfy server URL (falls back to the server's configured one).
   */
  ntfy_url?: string | null;
  /**
   * FCM / APNs device token, the ntfy topic, or (kind "unifiedpush") the full endpoint URL the distributor gave the app.
   */
  target: string;
}
/**
 * Push delivery settings. Secrets are masked in GET responses; send the mask back to keep them.
 */
export interface PushSettings {
  apns_api_base?: string | null;
  apns_key_id?: string | null;
  /**
   * APNs token auth key (.p8 contents).
   */
  apns_key_pem?: string | null;
  apns_sandbox?: boolean;
  apns_team_id?: string | null;
  /**
   * The iOS bundle id.
   */
  apns_topic?: string | null;
  /**
   * Override for tests / proxies. Default `https://fcm.googleapis.com`.
   */
  fcm_api_base?: string | null;
  fcm_project_id?: string | null;
  /**
   * Firebase service-account JSON (FCM HTTP v1).
   */
  fcm_service_account?: string | null;
  /**
   * ntfy server (self-hosted on the compose network, or https://ntfy.sh).
   */
  ntfy_url?: string | null;
}
export interface RefreshRequest {
  refresh_token: string;
}
/**
 * Selective restore from a per-user or full backup (merge by default).
 */
export interface RestoreRequest {
  all?: boolean;
  /**
   * Id of a backup (see `GET /admin/backups`).
   */
  backup_id: string;
  /**
   * "merge" (default) adds what is missing; "replace" makes the user's view match the backup.
   */
  mode?: string | null;
  /**
   * Users to restore; ignored when `all` is true. Empty with `all=false` is an error.
   */
  user_ids?: string[];
}
export interface RestoreResponse {
  attachments_moved_back: number;
  conversations_created: number;
  messages_added: number;
  messages_hidden: number;
  messages_unhidden: number;
  users_restored: number;
  warnings: string[];
}
export interface ScheduleRequest {
  attachment_ids?: string[];
  body: string;
  /**
   * UTC ms; must be in the future.
   */
  due_ms: number;
  thread_root?: string | null;
}
export interface SectionRequest {
  collapsed?: boolean | null;
  name?: string | null;
  position?: number | null;
}
export interface SendMessage {
  /**
   * With `thread_root`: also show this reply in the conversation's main timeline ("Also send to channel"). Still one message.
   */
  also_send_to_channel?: boolean;
  attachment_ids?: string[];
  body: string;
  /**
   * Idempotency key generated by the client; retries never duplicate.
   */
  client_id: string;
  mentions?: string[];
  reply_to?: string | null;
  thread_root?: string | null;
}
export interface SessionInfo {
  client_version: string;
  created_ms: number;
  current: boolean;
  device_name: string;
  id: string;
  ip: string;
  last_seen_ms: number;
  revoked: boolean;
  user_id: string;
  username?: string;
}
/**
 * `PUT /conversations/{id}/avatar`. Owners and admins set the picture everyone in the channel or group sees.
 */
export interface SetConvAvatar {
  /**
   * Attachment id of an uploaded image (`purpose=avatar`); an empty string removes the picture.
   */
  avatar: string;
}
export interface SetupRequest {
  display_name: string;
  email?: string | null;
  password: string;
  storage?: StorageConfig | null;
  username: string;
}
export interface StorageTestResponse {
  message: string;
  ok: boolean;
}
/**
 * A thread root plus its replies (M-05).
 */
export interface ThreadView {
  replies: Message[];
  summary: ThreadSummary;
}
export interface TokenPair {
  access_token: string;
  /**
   * Seconds until `access_token` expires.
   */
  expires_in: number;
  refresh_token: string;
}
export interface UpdateBotRequest {
  /**
   * Attachment id of an image the admin uploaded with `purpose=avatar`; empty string clears the bot's picture.
   */
  avatar?: string | null;
  description?: string | null;
  disabled?: boolean | null;
  display_name?: string | null;
  /**
   * Job-title line shown under the bot's name (for example "Deploy bot").
   */
  title?: string | null;
}
/**
 * `PATCH /conversations/{id}` (admins and the conversation's managers).
 */
export interface UpdateConversation {
  archived?: boolean | null;
  name?: string | null;
  topic?: string | null;
  /**
   * Channels only.
   */
  visibility?: ChannelVisibility | null;
}
/**
 * `PUT /admin/org-settings`: only the given fields change.
 */
export interface UpdateOrgSettings {
  channel_creation?: ChannelCreation | null;
  channel_pins?: ChannelPins | null;
}
export interface UpdateOutgoingWebhook {
  conv_id?: string | null;
  enabled?: boolean | null;
  events?: string[] | null;
  name?: string | null;
  /**
   * True generates a new secret.
   */
  regenerate_secret?: boolean | null;
  url?: string | null;
}
export interface UploadResponse {
  attachment: Attachment;
}
export interface UserGroupRequest {
  handle?: string | null;
  member_ids?: string[] | null;
  name?: string | null;
}
/**
 * A conversation as shown in the admin viewer (any conversation, including other people's DMs).
 */
export interface ViewerConversation {
  archived?: boolean;
  /**
   * The picture of a channel or group DM that everyone sees (an attachment id with purpose `avatar`).
   */
  avatar?: string | null;
  created_ms: number;
  id: string;
  kind: ConversationKind;
  last_message?: Message | null;
  last_message_ts?: number | null;
  /**
   * Highest message sequence number in this conversation.
   */
  last_seq: number;
  members: Member[];
  message_count: number;
  name?: string | null;
  /**
   * Display title (the other person's name for DMs when viewing as a user).
   */
  title: string;
  topic?: string | null;
  /**
   * `public` or `private` (channels only; everything else is `private`). Channels that existed before 1.1.0 are `private`.
   */
  visibility?: ChannelVisibility & string;
}
/**
 * Admin viewer: a page of a conversation's messages plus reply counts for the thread badges.
 */
export interface ViewerHistory {
  has_more_after: boolean;
  has_more_before: boolean;
  messages: Message[];
  /**
   * Thread root id -> number of replies.
   */
  reply_counts: {
    [k: string]: number;
  };
}
export interface WebhooksOverview {
  incoming: IncomingWebhook[];
  outgoing: OutgoingWebhook[];
}
export interface IncomingWebhook {
  bot_user_id: string;
  conv_id: string;
  conv_name?: string;
  created_ms: number;
  enabled: boolean;
  id: string;
  last_used_ms?: number | null;
  name: string;
  /**
   * Path that accepts `POST {"text": "..."}` (relative to the server address).
   */
  path: string;
}
export interface OutgoingWebhook {
  /**
   * Only events of this conversation; absent means every conversation.
   */
  conv_id?: string | null;
  created_ms: number;
  enabled: boolean;
  /**
   * Event names, e.g. `message.created`; empty means all.
   */
  events: string[];
  failed: number;
  id: string;
  last_delivery_ms?: number | null;
  last_status?: string | null;
  name: string;
  pending: number;
  /**
   * HMAC-SHA256 key used for the `X-WorkChat-Signature` header.
   */
  secret: string;
  url: string;
}
