//! Phase 3 types: bots (SV-15), webhooks (SV-16), interactive forms and App Home (SV-17),
//! scheduled messages (SV-10) and the optional AI features (SV-21).

use crate::types::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ------------------------------------------------------------------ forms (SV-17)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FormFieldKind {
    Text,
    Multiline,
    Boolean,
    Select,
    MultiSelect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormOption {
    pub value: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormField {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FormFieldKind,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// String for text/select, bool for boolean, array of strings for multi_select.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<FormOption>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormButton {
    pub id: String,
    pub label: String,
    /// "primary", "danger" or "default".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// A button can open a modal form (A-03); the response is sent with this button id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Box<Form>>,
}

/// A JSON form a bot attaches to a message. Rendered natively as a card (or a modal when a
/// button carries its own form); the response goes back to the bot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Form {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FormField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<FormButton>,
    /// Label of the submit button shown when the form has fields (default "Submit").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit_label: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct FormSubmit {
    /// The pressed button; absent for the plain submit button of a form with fields.
    #[serde(default)]
    pub button_id: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, serde_json::Value>,
}

impl Form {
    /// The form whose fields a submission with `button_id` must satisfy.
    pub fn target(&self, button_id: Option<&str>) -> Option<&Form> {
        match button_id {
            None => Some(self),
            Some(b) => {
                let btn = self.buttons.iter().find(|x| x.id == b)?;
                Some(btn.form.as_deref().unwrap_or(self))
            }
        }
    }

    /// Checks `values` against the field definitions. Returns a plain-English problem.
    pub fn validate(&self, button_id: Option<&str>, values: &BTreeMap<String, serde_json::Value>) -> Result<(), String> {
        if let Some(b) = button_id {
            if !self.buttons.iter().any(|x| x.id == b) {
                return Err("unknown button".into());
            }
        }
        let has_modal = button_id.and_then(|b| self.buttons.iter().find(|x| x.id == b)).map(|b| b.form.is_some()).unwrap_or(false);
        // A plain button on a card with fields does not need the fields filled in.
        let plain_button = button_id.is_some() && !has_modal;
        let f = match self.target(button_id) {
            Some(f) => f,
            None => return Err("unknown button".into()),
        };
        if plain_button {
            return Ok(());
        }
        for field in &f.fields {
            let v = values.get(&field.id);
            let missing = match v {
                None | Some(serde_json::Value::Null) => true,
                Some(serde_json::Value::String(s)) => s.trim().is_empty(),
                Some(serde_json::Value::Array(a)) => a.is_empty(),
                _ => false,
            };
            if missing {
                if field.required {
                    return Err(format!("{} is required", if field.label.is_empty() { &field.id } else { &field.label }));
                }
                continue;
            }
            let v = v.unwrap();
            let ok = match field.kind {
                FormFieldKind::Text | FormFieldKind::Multiline => v.as_str().map(|s| s.chars().count() <= 4000).unwrap_or(false),
                FormFieldKind::Boolean => v.is_boolean(),
                FormFieldKind::Select => v.as_str().map(|s| field.options.iter().any(|o| o.value == s)).unwrap_or(false),
                FormFieldKind::MultiSelect => v
                    .as_array()
                    .map(|a| a.iter().all(|x| x.as_str().map(|s| field.options.iter().any(|o| o.value == s)).unwrap_or(false)))
                    .unwrap_or(false),
            };
            if !ok {
                return Err(format!("invalid value for {}", field.id));
            }
        }
        for k in values.keys() {
            if !f.fields.iter().any(|x| &x.id == k) {
                return Err(format!("unknown field {k}"));
            }
        }
        Ok(())
    }

    /// Structural checks done when a bot sends the form.
    pub fn check_shape(&self) -> Result<(), String> {
        fn walk(f: &Form, depth: u8) -> Result<(), String> {
            if depth > 1 {
                return Err("forms can be nested only one level (a button's modal)".into());
            }
            if f.id.trim().is_empty() || f.id.len() > 64 {
                return Err("form id is required (max 64 characters)".into());
            }
            if f.fields.len() > 20 || f.buttons.len() > 12 {
                return Err("too many fields or buttons".into());
            }
            let mut seen = std::collections::HashSet::new();
            for x in &f.fields {
                if x.id.trim().is_empty() || !seen.insert(x.id.clone()) {
                    return Err("field ids must be unique and not empty".into());
                }
                if matches!(x.kind, FormFieldKind::Select | FormFieldKind::MultiSelect) && x.options.is_empty() {
                    return Err(format!("field {} needs options", x.id));
                }
            }
            let mut seen = std::collections::HashSet::new();
            for b in &f.buttons {
                if b.id.trim().is_empty() || !seen.insert(b.id.clone()) {
                    return Err("button ids must be unique and not empty".into());
                }
                if let Some(sub) = &b.form {
                    walk(sub, depth + 1)?;
                }
            }
            Ok(())
        }
        walk(self, 0)
    }
}

/// App Home block (A-04).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HomeBlock {
    Header { text: String },
    Text { text: String },
    Divider {},
    Button { id: String, label: String, #[serde(default)] style: Option<String> },
    Form { form: Form },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BotHome {
    #[serde(default)]
    pub blocks: Vec<HomeBlock>,
}

/// What members see on a bot's profile (A-04).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BotProfile {
    pub user: User,
    pub description: String,
    pub home: BotHome,
}

// ------------------------------------------------------------------ bots (admin)

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BotWebhookInfo {
    pub url: String,
    pub has_secret: bool,
    pub pending_updates: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_ms: Option<Ts>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BotConvRef {
    pub id: String,
    pub name: String,
    pub kind: ConversationKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdminBot {
    pub user: User,
    pub description: String,
    /// Full bearer token (admins only). Use it as `/bot<token>/<method>` or `Authorization: Bearer <token>`.
    pub token: String,
    pub created_ms: Ts,
    pub conversations: Vec<BotConvRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook: Option<BotWebhookInfo>,
    /// Number of updates waiting for `getUpdates`.
    pub queued_updates: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateBotRequest {
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    /// Conversations (channels / group DMs) to add the bot to straight away.
    #[serde(default)]
    pub conv_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBotRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub disabled: Option<bool>,
    /// Attachment id of an image the admin uploaded with `purpose=avatar`; empty string clears the bot's picture.
    pub avatar: Option<String>,
    /// Job-title line shown under the bot's name (for example "Deploy bot").
    pub title: Option<String>,
}

// ------------------------------------------------------------------ webhooks (SV-16)

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IncomingWebhook {
    pub id: String,
    pub name: String,
    pub conv_id: String,
    #[serde(default)]
    pub conv_name: String,
    /// Path that accepts `POST {"text": "..."}` (relative to the server address).
    pub path: String,
    pub bot_user_id: String,
    pub enabled: bool,
    pub created_ms: Ts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_ms: Option<Ts>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateIncomingWebhook {
    pub name: String,
    pub conv_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutgoingWebhook {
    pub id: String,
    pub name: String,
    pub url: String,
    /// HMAC-SHA256 key used for the `X-WorkChat-Signature` header.
    pub secret: String,
    /// Event names, e.g. `message.created`; empty means all.
    pub events: Vec<String>,
    /// Only events of this conversation; absent means every conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conv_id: Option<String>,
    pub enabled: bool,
    pub created_ms: Ts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_delivery_ms: Option<Ts>,
    pub pending: i64,
    pub failed: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateOutgoingWebhook {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub conv_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateOutgoingWebhook {
    pub name: Option<String>,
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub conv_id: Option<String>,
    pub enabled: Option<bool>,
    /// True generates a new secret.
    pub regenerate_secret: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WebhooksOverview {
    pub incoming: Vec<IncomingWebhook>,
    pub outgoing: Vec<OutgoingWebhook>,
}

// ------------------------------------------------------------------ scheduled messages (SV-10)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledState {
    Pending,
    Sent,
    Cancelled,
    Failed,
}

impl ScheduledState {
    pub fn as_str(self) -> &'static str {
        match self {
            ScheduledState::Pending => "pending",
            ScheduledState::Sent => "sent",
            ScheduledState::Cancelled => "cancelled",
            ScheduledState::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> ScheduledState {
        match s {
            "sent" => ScheduledState::Sent,
            "cancelled" => ScheduledState::Cancelled,
            "failed" => ScheduledState::Failed,
            _ => ScheduledState::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduledMessage {
    pub id: String,
    pub conv_id: String,
    pub user_id: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_root: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachment_ids: Vec<String>,
    pub due_ms: Ts,
    pub created_ms: Ts,
    pub state: ScheduledState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleRequest {
    pub body: String,
    /// UTC ms; must be in the future.
    pub due_ms: Ts,
    #[serde(default)]
    pub thread_root: Option<String>,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
}

// ------------------------------------------------------------------ AI (SV-21)

/// One configured AI provider as the admin panel sees it. The API key is never returned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    /// "anthropic" or "openai" (any OpenAI-compatible endpoint).
    pub kind: String,
    pub model: String,
    /// Overrides the provider's default API address (OpenAI-compatible servers, proxies).
    #[serde(default)]
    pub base_url: String,
    pub enabled: bool,
    pub has_key: bool,
    /// Last four characters of the key (only when the key is long enough for that to be safe).
    #[serde(default)]
    pub key_hint: String,
}

/// All providers plus which one serves which feature. `None` = the first enabled provider with a key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiOverview {
    pub providers: Vec<AiProvider>,
    #[serde(default)]
    pub search_provider: Option<String>,
    #[serde(default)]
    pub recap_provider: Option<String>,
    /// Shown by the admin panel: message text is sent to the provider.
    pub notice: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AiProviderInput {
    pub name: String,
    /// "anthropic" or "openai" ("openai-compatible" is accepted as an alias).
    pub kind: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub api_key: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AiProviderUpdate {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// A new key; `None` or an empty string keeps the stored one.
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

/// Which provider serves which feature. An empty string clears the choice (= automatic).
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AiDefaultsUpdate {
    pub search_provider: Option<String>,
    pub recap_provider: Option<String>,
}

pub const AI_NOTICE: &str = "When AI features are on, the text of messages that match a search or belong to a recap is sent to the provider you configure here. Leave this off if that is not acceptable.";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiSearchRequest {
    pub query: String,
    /// Also ask the model for a short answer based on the top hits.
    #[serde(default)]
    pub answer: bool,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct AiInterpretation {
    pub q: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_conv: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_ms: Option<Ts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_ms: Option<Ts>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiSearchResponse {
    pub interpreted: AiInterpretation,
    pub results: SearchResponse,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AiRecapRequest {
    pub conv_id: String,
    /// Only messages after this time (UTC ms); default the last 24 hours.
    #[serde(default)]
    pub since_ms: Option<Ts>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiRecapResponse {
    pub summary: String,
    pub message_count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiTestResponse {
    pub ok: bool,
    pub message: String,
}
