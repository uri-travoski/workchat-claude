//! The page a one-time login link opens in a browser: `GET /l/<token>`.
//!
//! It checks the token without using it up, then tries to open the installed app (Android intent link, or the
//! `workchat://` scheme elsewhere). If the app is missing it offers downloads and manual steps. Nothing is loaded from
//! other sites and the page is never cached or sent as a referrer, so the token stays between the server and the browser.

use crate::auth::sha256_hex;
use crate::db::now_ms;
use crate::routes::admin::public_base;
use crate::state::St;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, request::Parts, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rusqlite::OptionalExtension;

const ICON: &str = include_str!("landing_icon.svg");
const FAVICON_PNG: &[u8] = include_bytes!("favicon.png");
const FAVICON_ICO: &[u8] = include_bytes!("favicon.ico");
const APPLE_TOUCH: &[u8] = include_bytes!("apple-touch-icon.png");
pub const ANDROID_PACKAGE: &str = "app.workchat";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenState {
    Valid,
    Used,
    Expired,
    Invalid,
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn pct(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn headers(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, max-age=0"));
    h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::HeaderName::from_static("x-robots-tag"), HeaderValue::from_static("noindex, nofollow"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
    );
    r
}

async fn token_state(st: &St, token: &str) -> TokenState {
    if !proto::rules::is_valid_token(token) {
        return TokenState::Invalid;
    }
    let th = sha256_hex(token);
    let now = now_ms();
    let row = st
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT expires_ms, used_ms FROM login_links WHERE token_hash=?1", [&th], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))).optional()?)
        })
        .await
        .ok()
        .flatten();
    match row {
        None => TokenState::Invalid,
        Some((_, Some(_))) => TokenState::Used,
        Some((exp, None)) if exp <= now => TokenState::Expired,
        Some(_) => TokenState::Valid,
    }
}

/// Installers found in the downloads folder: (label, file name).
pub fn downloads(st: &St) -> Vec<(&'static str, String)> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(&st.cfg.downloads_dir) else { return out };
    let mut names: Vec<String> = rd.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).filter_map(|e| e.file_name().into_string().ok()).collect();
    names.sort();
    for n in names {
        let l = n.to_lowercase();
        let label = if l.ends_with(".apk") {
            "Android"
        } else if l.ends_with(".appimage") || l.ends_with(".deb") || l.ends_with(".rpm") {
            "Linux"
        } else if l.ends_with(".exe") || l.ends_with(".msi") {
            "Windows"
        } else {
            continue;
        };
        out.push((label, n));
    }
    out
}

fn page(title: &str, body: &str, script: &str) -> Response {
    let html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><meta name="robots" content="noindex"><meta name="color-scheme" content="dark light"><link rel="icon" type="image/svg+xml" href="/favicon.svg"><link rel="icon" type="image/png" sizes="64x64" href="/favicon.png"><link rel="apple-touch-icon" href="/apple-touch-icon.png"><title>{title}</title>
<style>
:root{{--bg:#0f1724;--card:#19273f;--fg:#e6ecf5;--mut:#93a0b8;--acc:#4f8ef0;--on:#08111f;--line:#2c3e5f;--bad:#ff8a80}}
@media (prefers-color-scheme:light){{:root{{--bg:#f4f6fa;--card:#fff;--fg:#19273f;--mut:#5b6b86;--acc:#21375b;--on:#fff;--line:#d6dde9;--bad:#b3261e}}}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;min-height:100vh;display:flex;justify-content:center;padding:24px 16px}}
main{{width:100%;max-width:440px}}.icon{{width:84px;height:84px;display:block;margin:8px auto 12px}}.icon svg{{width:100%;height:100%}}
h1{{font-size:26px;text-align:center;margin:0 0 4px}}.sub{{text-align:center;color:var(--mut);margin:0 0 20px}}
.card{{background:var(--card);border:1px solid var(--line);border-radius:14px;padding:18px;margin:0 0 14px}}
.btn{{display:flex;align-items:center;justify-content:center;width:100%;min-height:52px;line-height:1.25;padding:10px 12px;text-align:center;border-radius:12px;border:1px solid var(--line);background:transparent;color:var(--fg);font:inherit;font-weight:600;text-decoration:none;cursor:pointer;margin:0 0 10px}}
.btn.pri{{background:var(--acc);border-color:var(--acc);color:var(--on);font-size:18px}}h2{{font-size:16px;margin:0 0 8px}}
.err{{color:var(--bad);font-weight:600}}.mut{{color:var(--mut);font-size:14px}}code{{background:rgba(127,127,127,.18);padding:2px 6px;border-radius:6px;word-break:break-all}}ol{{padding-left:20px;margin:8px 0}}
.status{{text-align:center;color:var(--mut);min-height:24px;margin:0 0 14px}}
</style></head><body><main><div class="icon" aria-hidden="true">{icon}</div><h1>WorkChat</h1>{body}</main>{script}</body></html>"#,
        icon = ICON,
    );
    let mut r = (StatusCode::OK, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response();
    r = headers(r);
    r
}

fn sub(st: &St) -> String {
    if st.cfg.server_name.trim().is_empty() || st.cfg.server_name == "WorkChat" {
        return String::new();
    }
    format!(r#"<p class="sub">{}</p>"#, esc(&st.cfg.server_name))
}

fn download_card(st: &St, base: &str) -> String {
    let files = downloads(st);
    let mut h = String::from(r#"<div class="card" id="dl"><h2>Client downloads</h2>"#);
    if files.is_empty() {
        h += r#"<p class="mut">No installers are hosted on this server. Get the app (Android, Linux, Windows) from GitHub.</p>"#;
    } else {
        for (label, name) in files {
            h += &format!(r#"<a class="btn" href="{b}/downloads/{n}">Download for {l} <span class="mut">({n})</span></a>"#, b = esc(base), n = esc(&name), l = label);
        }
    }
    h += &format!(r#"<p class="mut" style="margin:0"><a id="dl-latest" href="{u}" rel="noreferrer noopener">Open latest release on GitHub</a></p></div>"#, u = esc(&st.cfg.release_url));
    h
}

pub async fn landing(State(st): State<St>, parts: Parts, Path(token): Path<String>) -> Response {
    let base = public_base(&st, &parts).await;
    let state = token_state(&st, &token).await;
    if state != TokenState::Valid {
        let msg = match state {
            TokenState::Used => "This login link has already been used.",
            TokenState::Expired => "This login link has expired.",
            _ => "This login link is not valid.",
        };
        let body = format!(r#"{name}<div class="card"><p class="err" role="alert">{msg}</p><p class="mut">Ask your administrator for a new login link. Each link works once and for a limited time.</p></div>{dl}"#, name = sub(&st), dl = download_card(&st, &base));
        let mut r = page("WorkChat login link", &body, "");
        *r.status_mut() = StatusCode::GONE;
        if state == TokenState::Invalid {
            *r.status_mut() = StatusCode::NOT_FOUND;
        }
        return r;
    }
    let app = proto::rules::make_app_link(&base, &token);
    let web = proto::rules::make_login_link(&base, &token);
    let intent = format!(
        "intent://login?server={}&token={}#Intent;scheme=workchat;package={};S.browser_fallback_url={};end",
        pct(&base),
        token,
        ANDROID_PACKAGE,
        pct(&format!("{base}/downloads"))
    );
    let body = format!(
        r#"{name}
<div class="card"><a class="btn pri" id="open" href="{app}" data-app="{app}" data-intent="{intent}" data-link="{web}">Open in WorkChat</a><p class="status" id="st" role="status">Opening the app...</p>
<p class="mut">This link signs you in once. It will stop working after you use it.</p></div>
{dl}
<div class="card"><h2>Sign in manually</h2><button class="btn" id="copy" type="button">Copy login link</button>
<ol class="mut"><li>Install and open WorkChat.</li><li>Enter the server address <code>{server}</code>.</li><li>Choose <b>Login link</b>, paste the copied link and tap <b>Sign in</b>.</li></ol></div>"#,
        name = sub(&st),
        app = esc(&app),
        intent = esc(&intent),
        web = esc(&web),
        server = esc(&base),
        dl = download_card(&st, &base),
    );
    let script = r#"<script>
(function(){
var b=document.getElementById('open'),st=document.getElementById('st'),c=document.getElementById('copy');
var android=/Android/i.test(navigator.userAgent);
function go(){var u=android?b.dataset.intent:b.dataset.app;st.textContent='Opening the app...';location.href=u}
b.addEventListener('click',function(e){e.preventDefault();go();setTimeout(function(){st.textContent='If nothing happened, install the app below, then tap Open again.'},2500)});
c.addEventListener('click',function(){var t=b.dataset.link;
 function done(){c.textContent='Copied'}
 if(navigator.clipboard&&navigator.clipboard.writeText){navigator.clipboard.writeText(t).then(done,fb)}else fb();
 function fb(){var a=document.createElement('textarea');a.value=t;document.body.appendChild(a);a.select();try{document.execCommand('copy');done()}catch(e){c.textContent='Copy failed'}a.remove()}});
setTimeout(go,700);
setTimeout(function(){if(!document.hidden)st.textContent='If the app did not open, install it below, then tap Open again.'},3500);
})();
</script>"#;
    page("Open WorkChat", &body, script)
}

/// `GET /downloads` lists the installers, `GET /downloads/<file>` serves one.
pub async fn downloads_index(State(st): State<St>, parts: Parts) -> Response {
    let base = public_base(&st, &parts).await;
    let body = format!(r#"{name}{dl}"#, name = sub(&st), dl = download_card(&st, &base));
    page("WorkChat downloads", &body, "")
}

pub async fn download_file(State(st): State<St>, Path(name): Path<String>) -> Response {
    if name.contains('/') || name.contains('\\') || name.starts_with('.') || !downloads(&st).iter().any(|(_, n)| *n == name) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(st.cfg.downloads_dir.join(&name)).await {
        Ok(bytes) => {
            let ct = if name.ends_with(".apk") { "application/vnd.android.package-archive" } else { "application/octet-stream" };
            let mut r = Response::new(Body::from(bytes));
            r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
            if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", name.replace('"', ""))) {
                r.headers_mut().insert(header::CONTENT_DISPOSITION, v);
            }
            r
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn static_asset(ct: &'static str, body: &'static [u8]) -> Response {
    let mut r = Response::new(Body::from(body));
    r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    r
}

/// The WorkChat icon for browsers that ask for `/favicon.ico` and for the landing and downloads pages.
pub async fn favicon_svg() -> Response {
    static_asset("image/svg+xml", ICON.as_bytes())
}
pub async fn favicon_png() -> Response {
    static_asset("image/png", FAVICON_PNG)
}
pub async fn favicon_ico() -> Response {
    static_asset("image/x-icon", FAVICON_ICO)
}
pub async fn apple_touch_icon() -> Response {
    static_asset("image/png", APPLE_TOUCH)
}
