//! Update checker: asks GitHub's "latest release" API, compares versions and picks the asset for this platform by the
//! release asset names (a contract, see docs/RELEASE.md). Downloads are streamed to disk and checked against the
//! release's `SHA256SUMS` when it has one.

use crate::error::{CoreError, Result};
use futures_util::StreamExt;
use ring::digest;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

pub const DEFAULT_API: &str = "https://api.github.com/repos/uri-travoski/workchat-claude/releases/latest";
pub const REPO_URL: &str = "https://github.com/uri-travoski/workchat-claude";

/// `1.2.3`, `v1.2.3` or `1.2.3-rc.1` (build metadata after `+` is ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let s = s.split('+').next()?;
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (s, None),
    };
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(Version { major, minor, patch, pre: pre.filter(|p| !p.is_empty()) })
}

fn cmp_pre(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(a), Ok(b)) => a.cmp(&b),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

impl Ord for Version {
    fn cmp(&self, o: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(o.major, o.minor, o.patch)).then_with(|| match (&self.pre, &o.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => cmp_pre(a, b),
        })
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Platform {
    /// `android`, `ios`, `windows`, `linux`, `macos`.
    pub os: String,
    /// `x86_64` or `aarch64`.
    pub arch: String,
    pub portable: bool,
    pub appimage: bool,
    /// Linux package format of this system: `deb`, `rpm`, `flatpak`.
    pub package: Option<String>,
}

impl Platform {
    pub fn detect(portable: bool) -> Platform {
        let os = std::env::consts::OS.to_string();
        let package = if os == "linux" { linux_package() } else { None };
        Platform { os, arch: std::env::consts::ARCH.to_string(), portable, appimage: std::env::var_os("APPIMAGE").is_some(), package }
    }
    pub fn label(&self) -> &'static str {
        match self.os.as_str() {
            "android" => "Android",
            "ios" => "iOS",
            "windows" => "Windows",
            "linux" => "Linux",
            "macos" => "macOS",
            _ => "this device",
        }
    }
}

fn linux_package() -> Option<String> {
    if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
        return Some("flatpak".into());
    }
    if Path::new("/etc/debian_version").exists() {
        return Some("deb".into());
    }
    if ["/etc/redhat-release", "/etc/fedora-release", "/etc/SuSE-release", "/etc/suse-release"].iter().any(|p| Path::new(p).exists()) {
        return Some("rpm".into());
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// The asset for `p` among a release's assets. Names are the contract in docs/RELEASE.md. `None` means "open the release page".
pub fn pick_asset(assets: &[Asset], version: &str, p: &Platform) -> Option<Asset> {
    let v = version.trim().trim_start_matches(['v', 'V']);
    let name = match p.os.as_str() {
        "android" => format!("WorkChat-{v}-android-{}.apk", if p.arch == "aarch64" { "arm64" } else { "x86_64" }),
        "windows" => {
            let a = if p.arch == "aarch64" { "arm64" } else { "x64" };
            format!("WorkChat-{v}-windows-{a}-{}", if p.portable { "portable.exe" } else { "setup.exe" })
        }
        "linux" if p.arch == "x86_64" || p.arch == "aarch64" => {
            // Debian says amd64 / arm64; every other Linux asset uses the kernel's names (x86_64 / aarch64).
            let (deb, arch) = if p.arch == "aarch64" { ("arm64", "aarch64") } else { ("amd64", "x86_64") };
            match (p.appimage, p.package.as_deref()) {
                (true, _) => format!("WorkChat-{v}-linux-{arch}.AppImage"),
                (_, Some("deb")) => format!("WorkChat-{v}-linux-{deb}.deb"),
                (_, Some("rpm")) => format!("WorkChat-{v}-linux-{arch}.rpm"),
                (_, Some("flatpak")) => format!("WorkChat-{v}-linux-{arch}.flatpak"),
                _ => format!("WorkChat-{v}-linux-{arch}.AppImage"),
            }
        }
        _ => return None,
    };
    assets.iter().find(|a| a.name == name).cloned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub notes: String,
    pub page: String,
    pub published: String,
    pub asset: Option<Asset>,
    /// What the Update button does here: `install` (Android), `installer` / `portable` (Windows), `appimage`, `package` (Linux deb, rpm, flatpak) or `page` (open the release page).
    pub action: String,
    pub checksum_url: Option<String>,
    pub platform: String,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}
#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

pub fn action_for(p: &Platform, asset: &Option<Asset>) -> &'static str {
    match (p.os.as_str(), asset) {
        (_, None) => "page",
        ("android", _) => "install",
        ("windows", _) => if p.portable { "portable" } else { "installer" },
        ("linux", _) => if p.appimage { "appimage" } else { "package" },
        _ => "page",
    }
}

pub fn parse_release(json: &str, current: &str, p: &Platform) -> Result<UpdateInfo> {
    let r: RawRelease = serde_json::from_str(json).map_err(|_| CoreError::local("The update server sent something unexpected."))?;
    let latest = r.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    if parse_version(&latest).is_none() {
        return Err(CoreError::local(format!("The release name \"{}\" is not a version.", r.tag_name)));
    }
    let assets: Vec<Asset> = r.assets.iter().map(|a| Asset { name: a.name.clone(), url: a.browser_download_url.clone(), size: a.size }).collect();
    let asset = pick_asset(&assets, &latest, p);
    let checksum_url = assets.iter().find(|a| a.name == "SHA256SUMS").map(|a| a.url.clone());
    Ok(UpdateInfo {
        current: current.to_string(),
        newer: is_newer(&latest, current),
        notes: r.body.unwrap_or_default().trim().to_string(),
        page: r.html_url.unwrap_or_else(|| format!("{REPO_URL}/releases/tag/v{latest}")),
        published: r.published_at.unwrap_or_default(),
        action: action_for(p, &asset).to_string(),
        asset,
        latest,
        checksum_url,
        platform: p.label().to_string(),
    })
}

/// The API address: the `WORKCHAT_UPDATE_API` environment variable or the developer override saved in the app, else GitHub.
/// Anything other than GitHub is only accepted from those overrides, and https is required for GitHub.
pub fn api_url(saved_override: Option<String>) -> String {
    std::env::var("WORKCHAT_UPDATE_API").ok().filter(|s| !s.trim().is_empty()).or(saved_override.filter(|s| !s.trim().is_empty())).unwrap_or_else(|| DEFAULT_API.to_string())
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder().user_agent(format!("WorkChat/{}", env!("CARGO_PKG_VERSION"))).connect_timeout(std::time::Duration::from_secs(15)).timeout(std::time::Duration::from_secs(20)).build().map_err(|e| CoreError::Network(e.to_string()))
}

pub async fn check(api: &str, current: &str, p: &Platform) -> Result<UpdateInfo> {
    let resp = client()?.get(api).header("Accept", "application/vnd.github+json").send().await.map_err(|e| CoreError::Network(format!("Could not reach the update server: {e}")))?;
    let st = resp.status();
    if st.as_u16() == 404 {
        return Err(CoreError::local("No release has been published yet."));
    }
    if !st.is_success() {
        return Err(CoreError::Network(if st.as_u16() == 403 || st.as_u16() == 429 { "GitHub is limiting requests right now. Try again later.".into() } else { format!("The update server answered HTTP {}.", st.as_u16()) }));
    }
    let text = resp.text().await.map_err(|e| CoreError::Network(e.to_string()))?;
    parse_release(&text, current, p)
}

/// The expected hash of `name` in a `sha256sum` style listing (`<hex>  <name>`).
pub fn sha_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, n) = (it.next()?, it.next()?);
        (n.trim_start_matches('*') == name && h.len() == 64).then(|| h.to_ascii_lowercase())
    })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Streams `url` to `dest` (through a `.part` file), reporting progress, then checks the hash when `expected` is known.
pub async fn download(url: &str, dest: &Path, expected: Option<&str>, cancel: &AtomicBool, progress: impl Fn(u64, u64)) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let http = reqwest::Client::builder().user_agent(format!("WorkChat/{}", env!("CARGO_PKG_VERSION"))).connect_timeout(std::time::Duration::from_secs(15)).read_timeout(std::time::Duration::from_secs(30)).build().map_err(|e| CoreError::Network(e.to_string()))?;
    let resp = http.get(url).send().await.map_err(|e| CoreError::Network(format!("Download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(CoreError::Network(format!("Download failed (HTTP {}).", resp.status().as_u16())));
    }
    let total = resp.content_length().unwrap_or(0);
    if let Some(d) = dest.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    let part = dest.with_extension("part");
    let mut f = tokio::fs::File::create(&part).await?;
    let mut ctx = digest::Context::new(&digest::SHA256);
    let mut got = 0u64;
    let mut stream = resp.bytes_stream();
    let mut last = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        if cancel.load(AtomicOrdering::SeqCst) {
            drop(f);
            let _ = tokio::fs::remove_file(&part).await;
            return Err(CoreError::local("Download cancelled."));
        }
        let chunk = chunk.map_err(|e| CoreError::Network(format!("Download interrupted: {e}")))?;
        ctx.update(&chunk);
        f.write_all(&chunk).await?;
        got += chunk.len() as u64;
        if last.elapsed() >= std::time::Duration::from_millis(150) {
            last = std::time::Instant::now();
            progress(got, total);
        }
    }
    f.flush().await?;
    drop(f);
    progress(got, total.max(got));
    if total > 0 && got != total {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(CoreError::Network("The download is incomplete.".into()));
    }
    if let Some(want) = expected {
        let have = hex(ctx.finish().as_ref());
        if !have.eq_ignore_ascii_case(want) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(CoreError::local("The downloaded file does not match the published checksum, so it was discarded."));
        }
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

pub async fn fetch_checksum(sums_url: &str, name: &str) -> Option<String> {
    let text = client().ok()?.get(sums_url).send().await.ok()?.text().await.ok()?;
    sha_for(&text, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plat(os: &str, arch: &str) -> Platform {
        Platform { os: os.into(), arch: arch.into(), portable: false, appimage: false, package: None }
    }
    fn assets(v: &str) -> Vec<Asset> {
        [
            "android-arm64.apk", "android-x86_64.apk", "android-arm64-unsigned.apk", "windows-x64-setup.exe", "windows-arm64-setup.exe", "windows-x64-portable.exe", "windows-arm64-portable.exe",
            "linux-amd64.deb", "linux-x86_64.rpm", "linux-x86_64.AppImage", "linux-x86_64.flatpak", "ios-unsigned.ipa",
            "linux-arm64.deb", "linux-aarch64.rpm", "linux-aarch64.AppImage", "linux-aarch64.flatpak",
        ]
        .iter()
        .map(|s| Asset { name: format!("WorkChat-{v}-{s}"), url: format!("http://x/{s}"), size: 1 })
        .chain([Asset { name: "SHA256SUMS".into(), url: "http://x/sums".into(), size: 1 }])
        .collect()
    }

    #[test]
    fn versions() {
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("1.1.0", "1.1.0"));
        assert!(!is_newer("1.0.9", "1.1.0"));
        assert!(!is_newer("1.1.0-rc.1", "1.1.0"), "a pre-release is older than its release");
        assert!(is_newer("1.1.0", "1.1.0-rc.1"));
        assert!(is_newer("1.1.0-rc.2", "1.1.0-rc.1"));
        assert!(is_newer("1.1.0-rc.10", "1.1.0-rc.9"));
        assert!(is_newer("2", "1.9.9"));
        assert!(!is_newer("garbage", "1.0.0"));
        assert!(!is_newer("1.2.3.4", "1.0.0"));
        assert_eq!(parse_version("v1.2.3+build5"), Some(Version { major: 1, minor: 2, patch: 3, pre: None }));
    }

    #[test]
    fn asset_names_per_platform() {
        let a = assets("1.2.0");
        let n = |p: &Platform| pick_asset(&a, "v1.2.0", p).map(|x| x.name);
        assert_eq!(n(&plat("android", "aarch64")).as_deref(), Some("WorkChat-1.2.0-android-arm64.apk"));
        assert_eq!(n(&plat("android", "x86_64")).as_deref(), Some("WorkChat-1.2.0-android-x86_64.apk"));
        assert_eq!(n(&plat("windows", "x86_64")).as_deref(), Some("WorkChat-1.2.0-windows-x64-setup.exe"));
        assert_eq!(n(&plat("windows", "aarch64")).as_deref(), Some("WorkChat-1.2.0-windows-arm64-setup.exe"));
        let mut pw = plat("windows", "x86_64");
        pw.portable = true;
        assert_eq!(n(&pw).as_deref(), Some("WorkChat-1.2.0-windows-x64-portable.exe"));
        pw.arch = "aarch64".into();
        assert_eq!(n(&pw).as_deref(), Some("WorkChat-1.2.0-windows-arm64-portable.exe"));
        let mut l = plat("linux", "x86_64");
        l.appimage = true;
        assert_eq!(n(&l).as_deref(), Some("WorkChat-1.2.0-linux-x86_64.AppImage"));
        l.appimage = false;
        l.package = Some("deb".into());
        assert_eq!(n(&l).as_deref(), Some("WorkChat-1.2.0-linux-amd64.deb"));
        l.package = Some("rpm".into());
        assert_eq!(n(&l).as_deref(), Some("WorkChat-1.2.0-linux-x86_64.rpm"));
        l.package = Some("flatpak".into());
        assert_eq!(n(&l).as_deref(), Some("WorkChat-1.2.0-linux-x86_64.flatpak"));
        // arm64 Linux picks the arm64 / aarch64 assets for every kind
        let mut la = plat("linux", "aarch64");
        assert_eq!(n(&la).as_deref(), Some("WorkChat-1.2.0-linux-aarch64.AppImage"), "unknown package kind falls back to the AppImage");
        la.appimage = true;
        assert_eq!(n(&la).as_deref(), Some("WorkChat-1.2.0-linux-aarch64.AppImage"));
        la.appimage = false;
        for (kind, want) in [("deb", "linux-arm64.deb"), ("rpm", "linux-aarch64.rpm"), ("flatpak", "linux-aarch64.flatpak")] {
            la.package = Some(kind.into());
            assert_eq!(n(&la), Some(format!("WorkChat-1.2.0-{want}")));
        }
        assert_eq!(n(&plat("linux", "riscv64")), None, "unsupported CPU: release page");
        // an x86_64 box never gets an arm64 asset, even when only those exist
        let arm_only: Vec<Asset> = a.iter().filter(|x| x.name.contains("arm64.deb") || x.name.contains("aarch64")).cloned().collect();
        l.package = Some("deb".into());
        assert_eq!(pick_asset(&arm_only, "1.2.0", &l), None);
        assert_eq!(n(&plat("ios", "aarch64")), None);
        // unsigned Android files never match
        let only_unsigned: Vec<Asset> = a.iter().filter(|x| !x.name.ends_with("android-arm64.apk")).cloned().collect();
        assert_eq!(pick_asset(&only_unsigned, "1.2.0", &plat("android", "aarch64")), None);
    }

    #[test]
    fn release_json() {
        let json = serde_json::json!({
            "tag_name": "v1.2.0", "body": "## What's new\n- things\n", "html_url": "https://github.com/uri-travoski/workchat-claude/releases/tag/v1.2.0", "published_at": "2026-12-01T10:00:00Z",
            "assets": assets("1.2.0").iter().map(|a| serde_json::json!({"name": a.name, "browser_download_url": a.url, "size": a.size})).collect::<Vec<_>>()
        })
        .to_string();
        let i = parse_release(&json, "1.1.0", &plat("android", "x86_64")).unwrap();
        assert!(i.newer);
        assert_eq!((i.latest.as_str(), i.action.as_str(), i.checksum_url.as_deref()), ("1.2.0", "install", Some("http://x/sums")));
        assert!(i.notes.starts_with("## What's new"));
        let same = parse_release(&json, "1.2.0", &plat("ios", "aarch64")).unwrap();
        assert!(!same.newer);
        assert_eq!(same.action, "page");
        assert!(parse_release("{}", "1.0.0", &plat("linux", "x86_64")).is_err());
        assert!(parse_release(&json.replace("v1.2.0", "nightly"), "1.0.0", &plat("linux", "x86_64")).is_err());
    }

    #[test]
    fn checksums() {
        let sums = format!("{}  WorkChat-1.2.0-android-arm64.apk\n{} *other.bin\nbad line\n", "A".repeat(64), "b".repeat(64));
        assert_eq!(sha_for(&sums, "WorkChat-1.2.0-android-arm64.apk"), Some("a".repeat(64)));
        assert_eq!(sha_for(&sums, "other.bin"), Some("b".repeat(64)));
        assert_eq!(sha_for(&sums, "missing"), None);
        assert_eq!(hex(digest::digest(&digest::SHA256, b"abc").as_ref()), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[tokio::test]
    async fn download_checks_hash_and_progress() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let body = vec![7u8; 300_000];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let b = body.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let b = b.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = s.read(&mut buf).await;
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len());
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(&b).await;
                });
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let url = format!("http://127.0.0.1:{port}/f.bin");
        let good = hex(digest::digest(&digest::SHA256, &body).as_ref());
        let seen = std::sync::atomic::AtomicU64::new(0);
        let cancel = AtomicBool::new(false);
        download(&url, &dir.path().join("f.bin"), Some(&good), &cancel, |g, _| seen.store(g, AtomicOrdering::SeqCst)).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("f.bin")).unwrap(), body);
        assert_eq!(seen.load(AtomicOrdering::SeqCst), body.len() as u64);
        let err = download(&url, &dir.path().join("g.bin"), Some(&"0".repeat(64)), &cancel, |_, _| {}).await.unwrap_err();
        assert!(err.to_string().contains("checksum"));
        assert!(!dir.path().join("g.bin").exists() && !dir.path().join("g.part").exists());
        cancel.store(true, AtomicOrdering::SeqCst);
        assert!(download(&url, &dir.path().join("h.bin"), None, &cancel, |_, _| {}).await.is_err());
    }
}
