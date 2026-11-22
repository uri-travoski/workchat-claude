//! Shared harness of the server tests: an in-process server on an ephemeral port plus raw HTTP/WebSocket clients
//! (`workchat_testkit`). No client library is involved.
#![allow(dead_code, unused_imports)]
pub use proto::*;
pub use serde_json::{json, Value};
pub use workchat_server::config::Config;
pub use workchat_testkit::{eventually, uid, now_ms, webhook_signature, ApiErr, Client, HttpMethod, Res, Resp, Ws};

pub mod fakes;

pub const PW: &str = "password-123";

pub struct TestServer {
    pub url: String,
    pub dir: tempfile::TempDir,
    pub st: workchat_server::state::St,
    /// The owner, signed in through the setup wizard.
    pub admin: Client,
}

impl TestServer {
    pub async fn start() -> TestServer {
        Self::start_with(|_| {}).await
    }

    pub async fn start_with(f: impl FnOnce(&mut Config)) -> TestServer {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_test(dir.path().join("server"));
        f(&mut cfg);
        let st = workchat_server::build_state(cfg).await.unwrap();
        let url = workchat_server::serve_for_test(st.clone()).await.unwrap();
        let admin = Client::setup(
            &url,
            &SetupRequest { username: "admin".into(), display_name: "Admin".into(), password: "test-admin-pw".into(), email: Some("admin@example.com".into()), storage: None },
        )
        .await
        .unwrap();
        TestServer { url, dir, st, admin }
    }

    /// Makes every unused login link expired.
    pub async fn expire_login_links(&self) {
        self.st.db.call(|c| { c.execute("UPDATE login_links SET expires_ms=1", [])?; Ok(()) }).await.unwrap();
    }

    /// Creates a member with the password `PW` and signs them in (one device). Returns the signed-in client.
    pub async fn user(&self, name: &str) -> Client {
        self.admin.create_user(name, &capitalise(name), Some(PW)).await.unwrap();
        self.login(name).await
    }

    pub async fn login(&self, name: &str) -> Client {
        Client::login_device(&self.url, name, PW, &format!("test-{name}")).await.unwrap()
    }

    /// Admin sets Privacy options to Allow for the user.
    pub async fn allow_signout(&self, id: &str) {
        self.admin.update_user(id, &UpdateUserRequest { privacy_controls_enabled: Some(true), ..Default::default() }).await.unwrap();
    }

    pub async fn channel(&self, name: &str, members: &[&Client]) -> String {
        let ids: Vec<String> = members.iter().map(|m| m.user_id.clone()).collect();
        self.admin.create_channel(name, &ids).await.unwrap().conversation.id
    }
}

pub fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
