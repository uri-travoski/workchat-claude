//! GitHub release information used by the server's admin "Updates" card (and available to clients): version comparison,
//! the release JSON (GitHub's API shape), release-notes trimming. Pure functions, no network.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Where the newest release is read from. Replace with `WORKCHAT_UPDATE_URL`.
pub const DEFAULT_UPDATE_URL: &str = "https://api.github.com/repos/uri-travoski/workchat-claude/releases/latest";
/// The project's releases page (shown when there is nothing to install automatically).
pub const RELEASES_PAGE: &str = "https://github.com/uri-travoski/workchat-claude/releases";
const NOTES_MAX_CHARS: usize = 1500;

/// The release JSON could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadRelease;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Pre-release part after `-` (empty for a normal release).
    pub pre: String,
}

/// Parses `1.3.1`, `v1.3.1`, `1.3`, `1.3.1-rc.1` and `1.3.1+build5`. None for anything else.
pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim();
    let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
    let s = s.split('+').next().unwrap_or(s);
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, p.to_string()),
        None => (s, String::new()),
    };
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    let patch = it.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    if it.next().is_some() {
        return None;
    }
    Some(Version { major, minor, patch, pre })
}

fn cmp_pre(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater, // 1.3.1 > 1.3.1-rc.1
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
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
        (self.major, self.minor, self.patch).cmp(&(o.major, o.minor, o.patch)).then_with(|| cmp_pre(&self.pre, &o.pre))
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// True when `candidate` is a strictly newer version than `current` (`1.3.10` is newer than `1.3.9`; a leading `v` is
/// ignored). Unparseable versions are never newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseInfo {
    /// Version without the `v` (`1.3.1`).
    pub version: String,
    /// The tag as published (`v1.3.1`).
    pub tag: String,
    pub name: String,
    /// Release notes as plain text, trimmed.
    pub notes: String,
    pub html_url: String,
    pub published: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct RawRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

/// Markdown release notes as short plain text: heading marks and emphasis removed, bullets as dots, blank lines collapsed,
/// cut at about 1500 characters.
pub fn trim_notes(body: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in body.replace('\r', "").lines() {
        let t = line.trim_end();
        let t2 = t.trim_start();
        let mut s = if t2.starts_with('#') { t2.trim_start_matches('#').trim_start().to_string() } else { t.to_string() };
        if let Some(rest) = s.strip_prefix("* ").or_else(|| s.strip_prefix("- ")) {
            s = format!("\u{2022} {rest}");
        }
        let s = s.replace("**", "").replace('`', "");
        if s.trim().is_empty() {
            blank += 1;
            if blank > 1 || out.is_empty() {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(&s);
        out.push('\n');
    }
    let out = out.trim().to_string();
    if out.chars().count() > NOTES_MAX_CHARS {
        let cut: String = out.chars().take(NOTES_MAX_CHARS).collect();
        let cut = cut.rsplit_once(['\n', ' ']).map(|(a, _)| a.to_string()).unwrap_or(cut);
        format!("{}\u{2026}", cut.trim_end())
    } else {
        out
    }
}

fn convert(r: RawRelease) -> Option<ReleaseInfo> {
    if r.draft || r.prerelease {
        return None;
    }
    let v = parse_version(&r.tag_name)?;
    if !v.pre.is_empty() {
        return None;
    }
    Some(ReleaseInfo {
        version: r.tag_name.trim().trim_start_matches(['v', 'V']).to_string(),
        tag: r.tag_name.clone(),
        name: r.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
        notes: trim_notes(&r.body.unwrap_or_default()),
        html_url: r.html_url,
        published: r.published_at.unwrap_or_default(),
        assets: r.assets.into_iter().filter(|a| !a.browser_download_url.is_empty()).map(|a| Asset { name: a.name, url: a.browser_download_url, size: a.size }).collect(),
    })
}

/// Reads the API's answer: one release object (`/releases/latest`) or a list (`/releases`). Drafts and pre-releases are
/// skipped; of a list the highest version is returned. `Ok(None)` when nothing eligible is in it.
pub fn parse_releases(json: &str) -> Result<Option<ReleaseInfo>, BadRelease> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| BadRelease)?;
    let raws: Vec<RawRelease> = if v.is_array() {
        serde_json::from_value(v).map_err(|_| BadRelease)?
    } else if v.is_object() {
        vec![serde_json::from_value(v).map_err(|_| BadRelease)?]
    } else {
        return Err(BadRelease);
    };
    Ok(raws.into_iter().filter_map(convert).max_by(|a, b| parse_version(&a.version).cmp(&parse_version(&b.version))))
}

