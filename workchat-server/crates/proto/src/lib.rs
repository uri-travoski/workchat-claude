//! WorkChat API types.
//!
//! This crate is the single source of truth for every JSON shape exchanged between the
//! server, the clients, the CLI and the admin panel. `cargo run -p workchat-proto --bin gen-schema`
//! writes `schema/api.schema.json` from these types.

pub mod admin;
pub mod automation;
pub mod error;
pub mod events;
pub mod release;
pub mod rules;
pub mod types;
pub mod ws;

pub use admin::*;
pub use automation::*;
pub use error::*;
pub use events::*;
pub use types::*;
pub use ws::*;

/// Bumped on any breaking change of the HTTP/WebSocket API.
pub const API_VERSION: u32 = 1;
/// Oldest client version the server of this release accepts.
pub const MIN_CLIENT_VERSION: &str = "1.0.0";
/// Version of this release.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Default maximum upload size (100 MB).
pub const DEFAULT_MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;
/// URL prefix of the REST API.
pub const API_PREFIX: &str = "/api/v1";

/// Every public type, used only to produce one JSON Schema document with all definitions.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
pub struct AllTypes {
    pub error: ErrorBody,
    pub meta: ServerMeta,
    pub login_request: LoginRequest,
    pub link_login_request: LinkLoginRequest,
    pub login_response: LoginResponse,
    pub refresh_request: RefreshRequest,
    pub token_pair: TokenPair,
    pub me: Me,
    pub bootstrap: Bootstrap,
    pub event: Event,
    pub events: EventsResponse,
    pub client_ws: ClientWsMsg,
    pub server_ws: ServerWsMsg,
    pub create_conversation: CreateConversation,
    pub update_conversation: UpdateConversation,
    pub send_message: SendMessage,
    pub edit_message: EditMessage,
    pub history: HistoryResponse,
    pub search: SearchResponse,
    pub upload: UploadResponse,
    pub setup: SetupRequest,
    pub admin_create_user: CreateUserRequest,
    pub admin_create_user_response: CreateUserResponse,
    pub admin_update_user: UpdateUserRequest,
    pub admin_set_role: SetRoleRequest,
    pub storage_config: StorageConfig,
    pub storage_test: StorageTestResponse,
    pub job: JobStatus,
    pub clear_history: ClearHistoryResponse,
    pub sessions: Vec<SessionInfo>,
    pub admin_conversation: AdminConversation,
    pub backups: Vec<BackupInfo>,
    pub pin: Pin,
    pub bookmark: Bookmark,
    pub bookmark_request: BookmarkRequest,
    pub user_group: UserGroup,
    pub user_group_request: UserGroupRequest,
    pub conv_pref: ConvPref,
    pub conv_pref_request: ConvPrefRequest,
    pub section: Section,
    pub section_request: SectionRequest,
    pub thread_summary: ThreadSummary,
    pub thread_view: ThreadView,
    pub draft_item: DraftItem,
    pub draft_request: DraftRequest,
    pub forward_request: ForwardRequest,
    pub push_registration: PushRegistration,
    pub file_hit: FileHit,
    pub backup_settings: BackupSettings,
    pub backup_settings_update: BackupSettingsUpdate,
    pub restore_request: RestoreRequest,
    pub restore_response: RestoreResponse,
    pub fresh_restore_request: FreshRestoreRequest,
    pub fresh_restore_response: FreshRestoreResponse,
    pub viewer_conversation: ViewerConversation,
    pub viewer_history: ViewerHistory,
    pub delete_range: DateRangeDeleteRequest,
    pub admin_delete_messages: AdminDeleteMessagesRequest,
    pub admin_delete_messages_response: AdminDeleteMessagesResponse,
    pub push_settings: PushSettings,
    pub form: Form,
    pub form_submit: FormSubmit,
    pub bot_home: BotHome,
    pub bot_profile: BotProfile,
    pub admin_bot: AdminBot,
    pub create_bot: CreateBotRequest,
    pub update_bot: UpdateBotRequest,
    pub webhooks: WebhooksOverview,
    pub create_incoming_webhook: CreateIncomingWebhook,
    pub create_outgoing_webhook: CreateOutgoingWebhook,
    pub update_outgoing_webhook: UpdateOutgoingWebhook,
    pub scheduled: ScheduledMessage,
    pub schedule_request: ScheduleRequest,
    pub ai_overview: AiOverview,
    pub ai_provider: AiProvider,
    pub ai_provider_input: AiProviderInput,
    pub ai_provider_update: AiProviderUpdate,
    pub ai_defaults_update: AiDefaultsUpdate,
    pub public_url_settings: PublicUrlSettings,
    pub admin_updates: AdminUpdates,
    pub release_info: release::ReleaseInfo,
    pub public_url_update: PublicUrlUpdate,
    pub ai_search_request: AiSearchRequest,
    pub ai_search_response: AiSearchResponse,
    pub ai_recap_request: AiRecapRequest,
    pub ai_recap_response: AiRecapResponse,
    pub ai_test: AiTestResponse,
    pub counts: Counts,
    pub org_settings: OrgSettings,
    pub update_org_settings: UpdateOrgSettings,
    pub channel_browse_page: ChannelBrowsePage,
    pub channel_browse_item: ChannelBrowseItem,
    pub members_request: MembersRequest,
    pub thread_summaries: Vec<ThreadSummary>,
    pub file_hits: Vec<FileHit>,
    pub avatar_override: AvatarOverride,
    pub set_conv_avatar: SetConvAvatar,
    pub hide_message_response: HideMessageResponse,
}

/// The complete JSON Schema document for the API.
pub fn api_schema() -> schemars::schema::RootSchema {
    schemars::schema_for!(AllTypes)
}
