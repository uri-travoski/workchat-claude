use crate::config::Config;
use crate::db::Db;
use crate::hub::Hub;
use crate::storage::Storage;
use proto::{JobStatus, StorageConfig};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub struct AppState {
    pub cfg: Config,
    pub db: Db,
    pub hub: Hub,
    pub storage: RwLock<Arc<dyn Storage>>,
    pub storage_cfg: RwLock<StorageConfig>,
    pub limiter: RateLimiter,
    pub lockouts: Lockouts,
    pub secret: Vec<u8>,
    /// Real Argon2id hash used to equalise login timing for unknown users.
    pub dummy_hash: String,
    pub jobs: Mutex<HashMap<String, JobStatus>>,
    pub migrating: std::sync::atomic::AtomicBool,
    pub http: reqwest::Client,
    /// Serialises full backups.
    pub backup_lock: tokio::sync::Mutex<()>,
    /// Set by `shutdown()`; background workers exit when they see it (tests restart a server on the same data).
    pub stopped: std::sync::atomic::AtomicBool,
    /// The last answer of the GitHub release check (admin Updates card), cached for an hour.
    pub updates: tokio::sync::Mutex<Option<proto::AdminUpdates>>,
}

pub type St = Arc<AppState>;

impl AppState {
    pub fn shutdown(&self) {
        self.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn storage(&self) -> Arc<dyn Storage> {
        self.storage.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn storage_config(&self) -> StorageConfig {
        self.storage_cfg.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Simple token bucket per key.
pub struct RateLimiter {
    per_sec: f64,
    burst: f64,
    map: Mutex<HashMap<String, (f64, Instant)>>,
}

impl RateLimiter {
    pub fn new(per_sec: f64, burst: f64) -> Self {
        RateLimiter { per_sec, burst, map: Mutex::new(HashMap::new()) }
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if m.len() > 20_000 {
            m.retain(|_, (_, t)| now.duration_since(*t) < Duration::from_secs(120));
        }
        let e = m.entry(key.to_string()).or_insert((self.burst, now));
        let el = now.duration_since(e.1).as_secs_f64();
        e.0 = (e.0 + el * self.per_sec).min(self.burst);
        e.1 = now;
        if e.0 >= 1.0 {
            e.0 -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Failed-login tracking with temporary lockout, per username and per IP.
pub struct Lockouts {
    threshold: u32,
    lock: Duration,
    map: Mutex<HashMap<String, (u32, Instant, Option<Instant>)>>,
}

impl Lockouts {
    pub fn new(threshold: u32, lock_secs: u64) -> Self {
        Lockouts { threshold, lock: Duration::from_secs(lock_secs), map: Mutex::new(HashMap::new()) }
    }

    /// Seconds left of an active lockout for any of the keys.
    pub fn locked(&self, keys: &[String]) -> Option<u64> {
        let now = Instant::now();
        let m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        keys.iter()
            .filter_map(|k| m.get(k))
            .filter_map(|(_, _, until)| *until)
            .filter(|u| *u > now)
            .map(|u| (u - now).as_secs() + 1)
            .max()
    }

    pub fn fail(&self, keys: &[(String, u32)]) {
        let now = Instant::now();
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        for (k, mult) in keys {
            let e = m.entry(k.clone()).or_insert((0, now, None));
            if now.duration_since(e.1) > Duration::from_secs(900) {
                *e = (0, now, None);
            }
            e.0 += 1;
            if e.0 >= self.threshold * mult {
                e.2 = Some(now + self.lock);
                e.0 = 0;
            }
        }
    }

    pub fn success(&self, key: &str) {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
    }
}
