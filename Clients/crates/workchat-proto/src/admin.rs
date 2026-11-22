use crate::types::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdminUser {
    #[serde(flatten)]
    pub user: User,
    pub email: Option<String>,
    pub created_ms: Ts,
    pub privacy_controls_enabled: bool,
    pub hide_last_active: bool,
    pub password_set: bool,
    pub deleted: bool,
    pub session_count: i64,
    /// Always the real value for admins, regardless of `hide_last_active`.
    pub last_active_real_ms: Option<Ts>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    /// Leave empty to create the account with a random password and a one-time login link only.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    /// Also create a one-time login link (default true when no password is given).
    #[serde(default)]
    pub create_login_link: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LoginLink {
    /// Paste this whole link (or just the token) on the app's login page.
    pub link: String,
    pub token: String,
    pub expires_ms: Ts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateUserResponse {
    pub user: AdminUser,
    pub login_link: Option<LoginLink>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub role: Option<Role>,
    pub disabled: Option<bool>,
    pub email: Option<String>,
    pub title: Option<String>,
    pub phone: Option<String>,
    /// Turns the Privacy screen on for this user (SV-12).
    pub privacy_controls_enabled: Option<bool>,
}

/// Body of `PUT /admin/users/{id}/role` (owner only).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetRoleRequest {
    pub role: Role,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ResetPasswordRequest {
    /// Omit to generate a random password and a one-time login link.
    pub new_password: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResetPasswordResponse {
    pub login_link: Option<LoginLink>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClearHistoryResponse {
    pub backup_id: String,
    pub backup_key: String,
    pub messages_hidden: i64,
    pub attachments_moved: i64,
    pub sessions_revoked: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackupInfo {
    pub id: String,
    /// "user", "full" or "messages".
    pub kind: String,
    pub user_id: Option<String>,
    pub created_ms: Ts,
    pub reason: String,
    pub key: String,
    pub size: i64,
    pub attachments: i64,
    /// True when the backup file is encrypted with the backup passphrase (age).
    #[serde(default)]
    pub encrypted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DateRangeDeleteRequest {
    /// Start of the range (inclusive). Omit together with `before_ms` semantics for "everything before".
    #[serde(default)]
    pub from_ms: Ts,
    /// End of the range (inclusive). Ignored when `before_ms` is set.
    #[serde(default)]
    pub to_ms: Ts,
    /// "Delete before date": every message strictly older than this time.
    #[serde(default)]
    pub before_ms: Option<Ts>,
    /// Restrict to one conversation.
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// Also hide the messages for every other member (default false: their copy stays).
    #[serde(default)]
    pub also_other_copies: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdminConversation {
    #[serde(flatten)]
    pub conversation: Conversation,
    pub message_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MembersRequest {
    pub user_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum S3Provider {
    Aws,
    R2,
    B2,
    Minio,
    Other,
}

/// Attachment storage backend configuration. Only config, accounts and the text database live
/// on the server's disk when a cloud backend is selected.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Directory on the server disk (testing / small installs).
    Local {
        #[serde(default)]
        path: Option<String>,
    },
    /// Any S3 API: AWS S3, Cloudflare R2, Backblaze B2, MinIO.
    S3 {
        provider: S3Provider,
        #[serde(default)]
        endpoint: Option<String>,
        region: String,
        bucket: String,
        access_key_id: String,
        secret_access_key: String,
        #[serde(default)]
        prefix: Option<String>,
        #[serde(default)]
        path_style: bool,
        #[serde(default)]
        allow_http: bool,
    },
    /// Google Drive (OAuth refresh token, drive.file scope).
    Gdrive {
        client_id: String,
        client_secret: String,
        refresh_token: String,
        #[serde(default)]
        folder_name: Option<String>,
        /// Override for tests / proxies. Default `https://www.googleapis.com`.
        #[serde(default)]
        api_base: Option<String>,
        #[serde(default)]
        token_url: Option<String>,
    },
}

impl StorageConfig {
    pub fn kind(&self) -> &'static str {
        match self {
            StorageConfig::Local { .. } => "local",
            StorageConfig::S3 { provider, .. } => match provider {
                S3Provider::R2 => "r2",
                S3Provider::B2 => "b2",
                _ => "s3",
            },
            StorageConfig::Gdrive { .. } => "gdrive",
        }
    }

    pub const MASK: &'static str = "********";

    /// Copy with secrets replaced by a mask (for GET responses).
    pub fn masked(&self) -> StorageConfig {
        let mut c = self.clone();
        match &mut c {
            StorageConfig::S3 { secret_access_key, .. } => *secret_access_key = Self::MASK.into(),
            StorageConfig::Gdrive { client_secret, refresh_token, .. } => {
                *client_secret = Self::MASK.into();
                *refresh_token = Self::MASK.into();
            }
            StorageConfig::Local { .. } => {}
        }
        c
    }

    /// Fill masked secrets from `previous` when the same backend is being re-submitted.
    pub fn unmask_from(mut self, previous: &StorageConfig) -> StorageConfig {
        match (&mut self, previous) {
            (
                StorageConfig::S3 { secret_access_key, .. },
                StorageConfig::S3 { secret_access_key: prev, .. },
            ) if secret_access_key == Self::MASK => *secret_access_key = prev.clone(),
            (
                StorageConfig::Gdrive { client_secret, refresh_token, .. },
                StorageConfig::Gdrive { client_secret: pc, refresh_token: pr, .. },
            ) => {
                if client_secret == Self::MASK {
                    *client_secret = pc.clone();
                }
                if refresh_token == Self::MASK {
                    *refresh_token = pr.clone();
                }
            }
            _ => {}
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StorageTestResponse {
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StorageMigrateRequest {
    pub target: StorageConfig,
    /// Delete the files from the old backend after every file has been copied and verified.
    #[serde(default = "default_true")]
    pub delete_source: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StorageVerifyRequest {
    /// Re-read every file and compare its SHA-256 (slower). Default only checks existence and size.
    #[serde(default)]
    pub deep: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobStatus {
    pub id: String,
    pub kind: String,
    /// "running", "done" or "failed".
    pub state: String,
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub message: String,
    pub errors: Vec<String>,
    pub started_ms: Ts,
    pub finished_ms: Option<Ts>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StorageInfo {
    pub config: StorageConfig,
    pub attachments: i64,
    pub bytes: i64,
}

// ---------------------------------------------------------------- backups (SV-20)

/// Where and how often full backups run. The passphrase is never returned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackupSettings {
    pub enabled: bool,
    /// Hours between scheduled backups (24 = daily).
    pub interval_hours: u32,
    /// How many full backups to keep; older ones are deleted from the target.
    pub retention: u32,
    /// Backup target. `None` means "the attachment storage".
    pub target: Option<StorageConfig>,
    pub passphrase_set: bool,
    pub last_run_ms: Option<Ts>,
    pub last_status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct BackupSettingsUpdate {
    pub enabled: Option<bool>,
    pub interval_hours: Option<u32>,
    pub retention: Option<u32>,
    pub target: Option<StorageConfig>,
    /// Use the attachment storage as the target again.
    #[serde(default)]
    pub clear_target: bool,
    /// Set or change the encryption passphrase (min. 8 characters).
    pub passphrase: Option<String>,
}

/// Selective restore from a per-user or full backup (merge by default).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RestoreRequest {
    /// Id of a backup (see `GET /admin/backups`).
    pub backup_id: String,
    /// Users to restore; ignored when `all` is true. Empty with `all=false` is an error.
    #[serde(default)]
    pub user_ids: Vec<String>,
    #[serde(default)]
    pub all: bool,
    /// "merge" (default) adds what is missing; "replace" makes the user's view match the backup.
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RestoreResponse {
    pub users_restored: i64,
    pub conversations_created: i64,
    pub messages_added: i64,
    pub messages_unhidden: i64,
    pub messages_hidden: i64,
    pub attachments_moved_back: i64,
    pub warnings: Vec<String>,
}

/// Restore of a full backup onto a fresh (empty) server.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FreshRestoreRequest {
    /// Where the backups are kept; omit when `upload` is used.
    pub target: Option<StorageConfig>,
    /// Object key of the backup; default: the most recent one.
    #[serde(default)]
    pub key: Option<String>,
    pub passphrase: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FreshRestoreResponse {
    pub users: i64,
    pub conversations: i64,
    pub messages: i64,
    pub attachments: i64,
    /// Attachments found in the connected storage after the restore.
    pub attachments_resolvable: i64,
    pub storage: String,
}

// ---------------------------------------------------------------- admin chat viewer

/// A conversation as shown in the admin viewer (any conversation, including other people's DMs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewerConversation {
    #[serde(flatten)]
    pub conversation: Conversation,
    /// Display title (the other person's name for DMs when viewing as a user).
    pub title: String,
    pub message_count: i64,
    pub last_message: Option<Message>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AdminDeleteMessagesRequest {
    /// Specific messages; or leave empty and use `conv_id` plus a date range.
    #[serde(default)]
    pub message_ids: Vec<String>,
    #[serde(default)]
    pub conv_id: Option<String>,
    #[serde(default)]
    pub from_ms: Option<Ts>,
    #[serde(default)]
    pub to_ms: Option<Ts>,
    /// Delete everything strictly before this time (in `conv_id`).
    #[serde(default)]
    pub before_ms: Option<Ts>,
    /// "everyone" (tombstone for all) or "user" (only `user_id`'s copy disappears).
    pub scope: String,
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdminDeleteMessagesResponse {
    pub backup_id: String,
    pub messages_deleted: i64,
    pub attachments_moved: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AdminRestoreMessagesRequest {
    pub backup_id: String,
}

// ---------------------------------------------------------------- push (SV-13)

/// Push delivery settings. Secrets are masked in GET responses; send the mask back to keep them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PushSettings {
    /// ntfy server (self-hosted on the compose network, or https://ntfy.sh).
    pub ntfy_url: Option<String>,
    /// Firebase service-account JSON (FCM HTTP v1).
    pub fcm_service_account: Option<String>,
    pub fcm_project_id: Option<String>,
    /// Override for tests / proxies. Default `https://fcm.googleapis.com`.
    pub fcm_api_base: Option<String>,
    /// APNs token auth key (.p8 contents).
    pub apns_key_pem: Option<String>,
    pub apns_key_id: Option<String>,
    pub apns_team_id: Option<String>,
    /// The iOS bundle id.
    pub apns_topic: Option<String>,
    #[serde(default)]
    pub apns_sandbox: bool,
    pub apns_api_base: Option<String>,
}

impl PushSettings {
    pub const MASK: &'static str = "********";
    pub fn masked(&self) -> PushSettings {
        let mut c = self.clone();
        if c.fcm_service_account.is_some() {
            c.fcm_service_account = Some(Self::MASK.into());
        }
        if c.apns_key_pem.is_some() {
            c.apns_key_pem = Some(Self::MASK.into());
        }
        c
    }
    pub fn unmask_from(mut self, prev: &PushSettings) -> PushSettings {
        if self.fcm_service_account.as_deref() == Some(Self::MASK) {
            self.fcm_service_account = prev.fcm_service_account.clone();
        }
        if self.apns_key_pem.as_deref() == Some(Self::MASK) {
            self.apns_key_pem = prev.apns_key_pem.clone();
        }
        self
    }
}

/// Admin viewer: a page of a conversation's messages plus reply counts for the thread badges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewerHistory {
    pub messages: Vec<Message>,
    pub has_more_before: bool,
    pub has_more_after: bool,
    /// Thread root id -> number of replies.
    pub reply_counts: std::collections::BTreeMap<String, i64>,
}

/// The externally visible base URL used in login links (owner setting; the env var wins when set).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublicUrlSettings {
    /// The URL saved in the admin panel (empty when none).
    #[serde(default)]
    pub url: Option<String>,
    /// What links are built from right now.
    pub effective: String,
    /// "env" (WORKCHAT_PUBLIC_URL), "setting" or "request" (derived from the Host header).
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublicUrlUpdate {
    /// `null` or empty clears the setting.
    #[serde(default)]
    pub url: Option<String>,
}

/// The admin panel's "Updates" card: this server's version against the newest GitHub release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdminUpdates {
    /// This server's version.
    pub current: String,
    /// The newest published release (drafts and pre-releases skipped), even when it is not newer.
    #[serde(default)]
    pub latest: Option<crate::release::ReleaseInfo>,
    /// `latest` is newer than `current`.
    pub update_available: bool,
    /// When the server last asked GitHub (ms since the epoch).
    pub checked_ms: i64,
    /// Why the check failed (offline, rate limit...), shown instead of the release.
    #[serde(default)]
    pub error: Option<String>,
    /// The address that was checked.
    pub source: String,
}
