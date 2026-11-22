//! workchat-server library (the binary in `main.rs` only reads the environment and serves).

pub mod admin;
pub mod auth;
pub mod ai;
pub mod backup;
pub mod bots;
pub mod config;
pub mod db;
pub mod emit;
pub mod crypto;
pub mod error;
pub mod fullbackup;
pub mod hooks;
pub mod hub;
pub mod landing;
pub mod org;
pub mod pdfthumb;
pub mod push;
pub mod queries;
pub mod restore;
pub mod routes;
pub mod saved;
pub mod scheduled;
pub mod state;
pub mod storage;
pub mod users;

use crate::config::Config;
use crate::db::{now_ms, Db};
use crate::error::AppResult;
use crate::hub::Hub;
use crate::state::*;
use axum::http::{header, HeaderValue};
use axum::response::Redirect;
use axum::routing::get;
use axum::Router;
use proto::{Role, StorageConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

pub async fn build_state(cfg: Config) -> anyhow::Result<St> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    let db = Db::open(&cfg.data_dir.join("workchat.db"))?;
    let storage_cfg = db
        .with(|c| admin::load_storage_config(c))?
        .unwrap_or(StorageConfig::Local { path: None });
    let storage = storage::build_storage(&storage_cfg, &cfg.data_dir)?;
    let secret = db.with(|c| routes::account::ensure_secret(c))?;
    let dummy_hash = auth::hash_password("not-a-real-password-for-timing")?;
    let st: St = Arc::new(AppState {
        limiter: RateLimiter::new(cfg.rate_per_sec, cfg.rate_burst),
        lockouts: Lockouts::new(cfg.lockout_threshold, cfg.lockout_secs),
        cfg,
        db,
        hub: Hub::new(),
        storage: RwLock::new(storage),
        storage_cfg: RwLock::new(storage_cfg),
        secret,
        dummy_hash,
        jobs: Mutex::new(HashMap::new()),
        migrating: std::sync::atomic::AtomicBool::new(false),
        http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?,
        backup_lock: tokio::sync::Mutex::new(()),
        stopped: std::sync::atomic::AtomicBool::new(false),
        updates: tokio::sync::Mutex::new(None),
    });
    bootstrap_admin(&st)?;
    Ok(st)
}

/// `WORKCHAT_BOOTSTRAP_ADMIN=user:password` creates the owner on an empty database.
fn bootstrap_admin(st: &St) -> anyhow::Result<()> {
    let Some((u, p)) = st.cfg.bootstrap_admin.clone() else { return Ok(()) };
    let n: i64 = st.db.with(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?))?;
    if n > 0 {
        return Ok(());
    }
    let hash = auth::hash_password(&p)?;
    let email = std::env::var("WORKCHAT_BOOTSTRAP_EMAIL").ok().filter(|e| !e.is_empty());
    let hub = st.hub.clone();
    st.db.with(|c| {
        let mut em = emit::Emitter::new();
        users::create_user(
            c,
            &hub,
            users::NewUser { username: u.clone(), display_name: "Administrator".into(), password_hash: hash, password_set: true, role: Role::Owner, email, title: None, phone: None },
            &mut em,
        )?;
        Ok(())
    })?;
    tracing::info!("created owner account '{u}' from WORKCHAT_BOOTSTRAP_ADMIN");
    Ok(())
}

pub fn router(st: St) -> Router {
    let admin_dir = st.cfg.admin_dir.clone();
    let mut app = Router::new()
        .route("/healthz", get(routes::account::health))
        .route("/", get(|| async { Redirect::temporary("/admin/") }))
        .route("/favicon.svg", get(landing::favicon_svg))
        .route("/favicon.png", get(landing::favicon_png))
        .route("/favicon.ico", get(landing::favicon_ico))
        .route("/apple-touch-icon.png", get(landing::apple_touch_icon))
        .route("/l/{token}", get(landing::landing))
        .route("/downloads", get(landing::downloads_index))
        .route("/downloads/{name}", get(landing::download_file))
        .nest("/api/v1", routes::api_router())
        // Telegram-compatible bot endpoints (same shape as api.telegram.org).
        .route("/bot{token}/{method}", get(routes::botapi::telegram).post(routes::botapi::telegram).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/file/bot{token}/{*path}", get(routes::botapi::telegram_file))
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024));
    if admin_dir.join("index.html").exists() {
        let serve = ServeDir::new(&admin_dir).fallback(ServeFile::new(admin_dir.join("index.html")));
        app = app.nest_service("/admin", serve);
    }
    app.layer(axum::middleware::from_fn(log_server_errors))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")))
        .layer(SetResponseHeaderLayer::if_not_present(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")))
        .with_state(st)
}

/// Logs every 5xx answer with the method and the route pattern (never the real path or query: bot tokens and
/// webhook secrets travel there). The cause itself is logged where the error is created (`AppError::internal`).
async fn log_server_errors(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let method = req.method().clone();
    let route = req.extensions().get::<axum::extract::MatchedPath>().map(|m| m.as_str().to_string()).unwrap_or_else(|| "(unmatched)".into());
    let res = next.run(req).await;
    if res.status().is_server_error() {
        tracing::error!("{} {} -> {}", method, route, res.status().as_u16());
    }
    res
}

/// Prunes the per-user event log older than the retention period (clients behind it re-bootstrap).
pub async fn prune_events(st: &St) -> AppResult<i64> {
    let cutoff = now_ms() - st.cfg.event_retention_days * 24 * 3600 * 1000;
    st.db
        .call(move |c| {
            let max: Option<i64> = c.query_row("SELECT MAX(id) FROM events WHERE ts < ?1", [cutoff], |r| r.get(0))?;
            if let Some(m) = max {
                c.execute("DELETE FROM events WHERE id<=?1", [m])?;
                let cur = routes::chat::events_floor(c)?;
                if m > cur {
                    db::set_setting(c, "events_floor", &m.to_string())?;
                }
                return Ok(m);
            }
            Ok(0)
        })
        .await
}

/// Scheduled-message sender, webhook delivery worker and housekeeping.
pub fn spawn_workers(st: St) {
    scheduled::spawn(st.clone());
    hooks::spawn(st.clone());
}

pub fn spawn_background(st: St) {
    spawn_workers(st.clone());
    fullbackup::spawn_scheduler(st.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
            if let Err(e) = prune_events(&st).await {
                tracing::warn!("event pruning failed: {e}");
            }
        }
    });
}

/// Starts the server on an ephemeral port (used by tests). Returns the base URL.
pub async fn serve_for_test(st: St) -> anyhow::Result<String> {
    // WORKCHAT_TEST_BIND pins the address (screenshots and emulator runs need a known port).
    let bind = std::env::var("WORKCHAT_TEST_BIND").unwrap_or_else(|_| "127.0.0.1:0".into());
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let addr = listener.local_addr()?;
    spawn_workers(st.clone());
    let app = router(st);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await;
    });
    Ok(format!("http://{addr}"))
}
