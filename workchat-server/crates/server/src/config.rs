use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub admin_dir: PathBuf,
    pub public_url: Option<String>,
    /// Folder with installers the landing page offers (WORKCHAT_DOWNLOADS_DIR, default `<data>/downloads`).
    pub downloads_dir: PathBuf,
    /// Where client apps are downloaded from when no file is in the downloads folder.
    pub release_url: String,
    /// GitHub API address of the newest release, for the admin "Updates" card (WORKCHAT_UPDATE_URL; tests point it at a fake).
    pub update_url: String,
    pub server_name: String,
    pub max_upload_bytes: u64,
    pub ntfy_url: Option<String>,
    pub bootstrap_admin: Option<(String, String)>,
    pub lockout_threshold: u32,
    pub lockout_secs: u64,
    pub access_ttl_secs: i64,
    pub refresh_ttl_secs: i64,
    pub rate_per_sec: f64,
    pub rate_burst: f64,
    pub event_retention_days: i64,
    /// scrypt work factor (log2 N) for backup encryption; `None` lets age pick (~1 s). Tests lower it.
    pub backup_work_factor: Option<u8>,
    /// How often the scheduled-message and webhook workers look for work.
    pub worker_tick_ms: u64,
    /// First retry delay for webhook deliveries; later retries multiply by 3 (capped at one hour).
    pub webhook_retry_base_ms: u64,
    pub webhook_max_attempts: i64,
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> Config {
        let bootstrap_admin = env("WORKCHAT_BOOTSTRAP_ADMIN").and_then(|v| {
            let (u, p) = v.split_once(':')?;
            Some((u.to_string(), p.to_string()))
        });
        let admin_dir = env("WORKCHAT_ADMIN_DIR").map(PathBuf::from).unwrap_or_else(|| {
            for c in ["/app/admin", "admin-web/dist"] {
                if std::path::Path::new(c).join("index.html").exists() {
                    return PathBuf::from(c);
                }
            }
            PathBuf::from("/app/admin")
        });
        Config {
            bind: env("WORKCHAT_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            data_dir: env("WORKCHAT_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("./data")),
            admin_dir,
            public_url: env("WORKCHAT_PUBLIC_URL").map(|u| u.trim_end_matches('/').to_string()),
            downloads_dir: env("WORKCHAT_DOWNLOADS_DIR").map(PathBuf::from).unwrap_or_else(|| env("WORKCHAT_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("./data")).join("downloads")),
            release_url: env("WORKCHAT_RELEASE_URL").unwrap_or_else(|| "https://github.com/uri-travoski/workchat-claude/releases/latest".into()),
            update_url: env("WORKCHAT_UPDATE_URL").unwrap_or_else(|| proto::release::DEFAULT_UPDATE_URL.into()),
            server_name: env("WORKCHAT_NAME").unwrap_or_else(|| "WorkChat".into()),
            max_upload_bytes: env("WORKCHAT_MAX_UPLOAD_MB")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|mb| mb * 1024 * 1024)
                .unwrap_or(proto::DEFAULT_MAX_UPLOAD_BYTES),
            ntfy_url: env("WORKCHAT_NTFY_URL"),
            bootstrap_admin,
            lockout_threshold: env("WORKCHAT_LOCKOUT_THRESHOLD").and_then(|v| v.parse().ok()).unwrap_or(5),
            lockout_secs: env("WORKCHAT_LOCKOUT_SECS").and_then(|v| v.parse().ok()).unwrap_or(300),
            access_ttl_secs: env("WORKCHAT_ACCESS_TTL_SECS").and_then(|v| v.parse().ok()).unwrap_or(900),
            refresh_ttl_secs: 90 * 24 * 3600,
            rate_per_sec: env("WORKCHAT_RATE_PER_SEC").and_then(|v| v.parse().ok()).unwrap_or(40.0),
            rate_burst: env("WORKCHAT_RATE_BURST").and_then(|v| v.parse().ok()).unwrap_or(200.0),
            event_retention_days: env("WORKCHAT_EVENT_RETENTION_DAYS").and_then(|v| v.parse().ok()).unwrap_or(90),
            backup_work_factor: env("WORKCHAT_BACKUP_WORK_FACTOR").and_then(|v| v.parse().ok()),
            worker_tick_ms: env("WORKCHAT_WORKER_TICK_MS").and_then(|v| v.parse().ok()).unwrap_or(1000),
            webhook_retry_base_ms: env("WORKCHAT_WEBHOOK_RETRY_BASE_MS").and_then(|v| v.parse().ok()).unwrap_or(10_000),
            webhook_max_attempts: env("WORKCHAT_WEBHOOK_MAX_ATTEMPTS").and_then(|v| v.parse().ok()).unwrap_or(8),
        }
    }

    /// Defaults suitable for tests.
    pub fn for_test(data_dir: PathBuf) -> Config {
        Config {
            bind: "127.0.0.1:0".into(),
            data_dir: data_dir.clone(),
            admin_dir: PathBuf::from("/nonexistent"),
            public_url: None,
            downloads_dir: data_dir.join("downloads"),
            release_url: "https://example.invalid/releases".into(),
            update_url: "http://127.0.0.1:1/unreachable".into(),
            server_name: "WorkChat (test)".into(),
            max_upload_bytes: 5 * 1024 * 1024,
            ntfy_url: None,
            bootstrap_admin: None,
            lockout_threshold: 5,
            lockout_secs: 300,
            access_ttl_secs: 900,
            refresh_ttl_secs: 90 * 24 * 3600,
            rate_per_sec: 1000.0,
            rate_burst: 5000.0,
            event_retention_days: 90,
            backup_work_factor: Some(10),
            worker_tick_ms: 100,
            webhook_retry_base_ms: 200,
            webhook_max_attempts: 4,
        }
    }
}
