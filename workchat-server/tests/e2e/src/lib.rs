//! Helpers shared by the end-to-end tests (`tests/full.rs`, `tests/bots.rs`). See `run.sh`.
//!
//! The tests need docker and are skipped unless `WORKCHAT_E2E` is set (so `cargo test --workspace` stays docker-free).
use std::process::Command;
use std::time::Duration;

/// True when the run was started through `run.sh` (or with `WORKCHAT_E2E=1`).
pub fn enabled() -> bool {
    std::env::var("WORKCHAT_E2E").is_ok()
}

/// Runs a command and returns (success, stdout + stderr).
pub fn sh(cmd: &str, args: &[&str]) -> (bool, String) {
    let o = Command::new(cmd).args(args).output().expect("run command");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

/// Removes the named containers (and their anonymous volumes) and the docker network when dropped, pass or fail.
pub struct Cleanup {
    pub containers: Vec<String>,
    pub network: Option<String>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for n in &self.containers {
            let _ = sh("docker", &["rm", "-f", "-v", n]);
        }
        if let Some(n) = &self.network {
            let _ = sh("docker", &["network", "rm", n]);
        }
    }
}

/// Waits (up to a minute) until `url` answers with any HTTP response.
pub async fn wait_http(url: &str) {
    for _ in 0..240 {
        if let Ok(r) = reqwest::get(url).await {
            let _ = r.status();
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("{url} never answered");
}
