//! Pure rules shared by the server (authoritative) and clients (mirrors for UI).

use crate::types::{ConvPref, ConversationKind, Ts};
use url::Url;

/// SV-05 / M-08 edit rule. `other_posted_after` is true when a participant other than the author
/// has posted a message after the original in the same conversation.
///
/// * channel and self: always allowed
/// * dm and group_dm: allowed only while nobody else has replied
pub fn edit_allowed(kind: ConversationKind, other_posted_after: bool) -> bool {
    match kind {
        ConversationKind::Channel | ConversationKind::SelfNote => true,
        ConversationKind::Dm | ConversationKind::GroupDm => !other_posted_after,
    }
}

/// Compares dotted numeric versions ("1.0.0" < "1.2"). Non-numeric parts count as 0.
pub fn version_lt(a: &str, b: &str) -> bool {
    fn parts(s: &str) -> Vec<u64> {
        s.split(['.', '-', '+'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (mut pa, mut pb) = (parts(a), parts(b));
    let n = pa.len().max(pb.len());
    pa.resize(n, 0);
    pb.resize(n, 0);
    pa < pb
}

pub const LOGIN_LINK_SCHEME: &str = "workchat";

pub fn is_link_scheme(s: &str) -> bool {
    s == LOGIN_LINK_SCHEME
}

/// The one-time login link an administrator shares: a normal web URL on the server itself, `<server>/l/<token>`.
/// Opening it in a browser shows a landing page that opens the app; the apps also accept it pasted.
pub fn make_login_link(server_url: &str, token: &str) -> String {
    format!("{}/l/{}", server_url.trim_end_matches('/'), token)
}

/// The app deep link (`workchat://login?server=<url>&token=<token>`) the landing page hands to the installed app.
pub fn make_app_link(server_url: &str, token: &str) -> String {
    let mut u = Url::parse("workchat://login").expect("static url");
    u.query_pairs_mut().append_pair("server", server_url.trim_end_matches('/')).append_pair("token", token);
    u.to_string()
}

/// Whether a path segment is a well-formed login token (landing page route check).
pub fn is_valid_token(t: &str) -> bool {
    valid_token(t)
}

/// Finds a `workchat://` login link anywhere in `text` (a message, an e-mail, a field with
/// extra words around it) and returns just the link.
pub fn extract_login_link(text: &str) -> Option<String> {
    {
        let marker = format!("{LOGIN_LINK_SCHEME}://login");
        if let Some(i) = text.find(&marker) {
            let rest = &text[i..];
            let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>')).unwrap_or(rest.len());
            let l = rest[..end].trim_end_matches(|c| matches!(c, '.' | ',' | ';' | ')'));
            if parse_login_link(l).is_some() {
                return Some(l.to_string());
            }
        }
    }
    // A web link `http(s)://host[:port]/l/<token>`.
    let mut from = 0;
    while let Some(i) = text[from..].find("://") {
        let at = from + i;
        let start = text[..at].rfind(|c: char| !c.is_ascii_alphabetic()).map(|j| j + 1).unwrap_or(0);
        let rest = &text[start..];
        let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>')).unwrap_or(rest.len());
        let l = rest[..end].trim_end_matches(|c| matches!(c, '.' | ',' | ';' | ')'));
        if (l.starts_with("http://") || l.starts_with("https://")) && l.contains("/l/") && parse_login_link(l).is_some() {
            return Some(l.to_string());
        }
        from = at + 3;
    }
    None
}

/// What the user pasted on the login page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLink {
    pub server: Option<String>,
    pub token: String,
}

/// Accepts a full link (`workchat://login?server=..&token=..`, or an `https://host/?token=..`
/// style URL) or a bare token.
pub fn parse_login_link(input: &str) -> Option<ParsedLink> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(u) = Url::parse(s) {
        if u.has_host() || is_link_scheme(u.scheme()) {
            let mut server = None;
            let mut token = None;
            if (u.scheme() == "https" || u.scheme() == "http") && u.has_host() {
                if let Some(t) = u.path().strip_prefix("/l/").map(|t| t.trim_end_matches('/')).filter(|t| valid_token(t)) {
                    let server = format!("{}://{}{}", u.scheme(), u.host_str().unwrap_or(""), u.port().map(|p| format!(":{p}")).unwrap_or_default());
                    return Some(ParsedLink { server: Some(server), token: t.to_string() });
                }
            }
            for (k, v) in u.query_pairs() {
                match k.as_ref() {
                    "server" => server = Some(v.to_string()),
                    "token" | "t" => token = Some(v.to_string()),
                    _ => {}
                }
            }
            if let Some(token) = token.filter(|t| valid_token(t)) {
                if server.is_none() && (u.scheme() == "https" || u.scheme() == "http") {
                    server = Some(format!(
                        "{}://{}{}",
                        u.scheme(),
                        u.host_str().unwrap_or(""),
                        u.port().map(|p| format!(":{p}")).unwrap_or_default()
                    ));
                }
                return Some(ParsedLink { server, token });
            }
            return None;
        }
    }
    if valid_token(s) {
        Some(ParsedLink { server: None, token: s.to_string() })
    } else {
        None
    }
}

fn valid_token(t: &str) -> bool {
    t.len() >= 16 && t.len() <= 128 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Usernames: 2-32 chars, letters, digits, `.`, `_`, `-`.
pub fn valid_username(u: &str) -> bool {
    (2..=32).contains(&u.len())
        && u.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

// ---------------------------------------------------------------- permalinks (M-19)

/// Deep link to a message: `workchat://m?server=<url>&c=<conv>&m=<message>`.
pub fn make_permalink(server_url: &str, conv_id: &str, message_id: &str) -> String {
    let mut u = Url::parse("workchat://m").expect("static url");
    u.query_pairs_mut()
        .append_pair("server", server_url.trim_end_matches('/'))
        .append_pair("c", conv_id)
        .append_pair("m", message_id);
    u.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permalink {
    pub server: Option<String>,
    pub conv_id: String,
    pub message_id: String,
}

/// Validates and parses a permalink; ids must be plain hex/alphanumerics (deep links are untrusted input).
pub fn parse_permalink(input: &str) -> Option<Permalink> {
    let u = Url::parse(input.trim()).ok()?;
    if !is_link_scheme(u.scheme()) || u.host_str() != Some("m") {
        return None;
    }
    let mut server = None;
    let (mut c, mut m) = (None, None);
    for (k, v) in u.query_pairs() {
        match k.as_ref() {
            "server" => server = Some(v.to_string()),
            "c" => c = Some(v.to_string()),
            "m" => m = Some(v.to_string()),
            _ => {}
        }
    }
    let ok = |s: &str| !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let (c, m) = (c?, m?);
    if ok(&c) && ok(&m) {
        Some(Permalink { server, conv_id: c, message_id: m })
    } else {
        None
    }
}

// ---------------------------------------------------------------- notifications (N-03, NV-07)

/// Decides whether a new message should notify the user: honours Do Not Disturb, mute, and the
/// per-conversation level (all / mentions only / nothing). Shared by the server's push logic and
/// the clients' local notifications. `@channel`-style mentions count as mentions.
pub fn should_notify(pref: Option<&ConvPref>, now: Ts, presence_pref: &str, mentioned: bool) -> bool {
    if presence_pref == "dnd" {
        return false;
    }
    let default = ConvPref::default_for("");
    let p = pref.unwrap_or(&default);
    if p.is_muted(now) {
        return false;
    }
    match p.notify.as_str() {
        "nothing" => false,
        "mentions" => mentioned,
        _ => true,
    }
}

/// Mute durations offered in the UI (NV-07), as an absolute `mute_until_ms` value.
/// `tomorrow_ms` is the start of the next local day, computed by the caller (time zone aware).
pub fn mute_until(choice: &str, now: Ts, tomorrow_ms: Ts) -> Option<Ts> {
    match choice {
        "1h" => Some(now + 3_600_000),
        "8h" => Some(now + 8 * 3_600_000),
        "tomorrow" => Some(tomorrow_ms),
        "forever" => Some(-1),
        "off" => Some(0),
        _ => None,
    }
}

// ---------------------------------------------------------------- mentions (M-17, M-18)

/// `@word` tokens in a message body, lowercased and without the `@` (word characters, `.`, `-`, `_`).
pub fn at_tokens(body: &str) -> Vec<String> {
    let mut out = vec![];
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '@' && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')) {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_alphanumeric() || matches!(chars[j], '.' | '-' | '_')) {
                j += 1;
            }
            let mut tok: String = chars[i + 1..j].iter().collect();
            while tok.ends_with('.') || tok.ends_with('-') {
                tok.pop();
            }
            if !tok.is_empty() {
                out.push(tok.to_lowercase());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// True for the reserved broadcast handles.
pub fn is_broadcast(tok: &str) -> bool {
    matches!(tok, "channel" | "everyone" | "here")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConversationKind::*;

    #[test]
    fn edit_rule_channel_and_self_always() {
        assert!(edit_allowed(Channel, true));
        assert!(edit_allowed(Channel, false));
        assert!(edit_allowed(SelfNote, true));
    }

    #[test]
    fn edit_rule_dm_closes_after_reply() {
        assert!(edit_allowed(Dm, false));
        assert!(!edit_allowed(Dm, true));
        assert!(edit_allowed(GroupDm, false));
        assert!(!edit_allowed(GroupDm, true));
    }

    #[test]
    fn versions() {
        assert!(version_lt("1.0.0", "1.0.1"));
        assert!(version_lt("0.9.9", "1.0"));
        assert!(!version_lt("1.0.0", "1.0.0"));
        assert!(!version_lt("2.0", "1.9.9"));
    }

    #[test]
    fn login_link_roundtrip() {
        let tok = "abcd1234efgh5678ijkl";
        let link = make_login_link("https://chat.example.com/", tok);
        assert_eq!(link, format!("https://chat.example.com/l/{tok}"));
        let p = parse_login_link(&link).unwrap();
        assert_eq!(p.server.as_deref(), Some("https://chat.example.com"));
        assert_eq!(p.token, tok);
        let p = parse_login_link(&format!("http://192.168.1.5:8080/l/{tok}/")).unwrap();
        assert_eq!(p.server.as_deref(), Some("http://192.168.1.5:8080"));
        let link = make_app_link("https://chat.example.com/", tok);
        let p = parse_login_link(&link).unwrap();
        assert_eq!(p.server.as_deref(), Some("https://chat.example.com"));
        assert_eq!(p.token, tok);
    }

    #[test]
    fn login_link_variants() {
        let p = parse_login_link("  abcd1234efgh5678ijkl  ").unwrap();
        assert_eq!(p.server, None);
        let p = parse_login_link("https://1.2.3.4:8443/login?token=abcd1234efgh5678ijkl").unwrap();
        assert_eq!(p.server.as_deref(), Some("https://1.2.3.4:8443"));
        assert!(parse_login_link("hello world").is_none());
        assert!(parse_login_link("").is_none());
        assert!(parse_login_link("workchat://login?server=https%3A%2F%2Fx.org").is_none());
    }

    #[test]
    fn usernames() {
        assert!(valid_username("dan.smith"));
        assert!(!valid_username("a"));
        assert!(!valid_username("bad name"));
    }

    #[test]
    fn permalinks() {
        let l = make_permalink("https://chat.example.com/", "c1", "m2");
        let p = parse_permalink(&l).unwrap();
        assert_eq!(p, Permalink { server: Some("https://chat.example.com".into()), conv_id: "c1".into(), message_id: "m2".into() });
        assert!(parse_permalink("workchat://m?c=a/b&m=x").is_none());
        assert!(parse_permalink("https://evil.example/m?c=a&m=b").is_none());
        assert!(parse_permalink("workchat://login?c=a&m=b").is_none());
        assert!(parse_permalink("workchat://m?c=aa&m=bb").is_some());
    }

    #[test]
    fn link_found_inside_other_text() {
        let tok = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
        let l = format!("workchat://login?server=https%3A%2F%2Fchat.example.org&token={tok}");
        assert_eq!(extract_login_link(&format!("Hi! Open {l}. Thanks")), Some(l.clone()));
        assert_eq!(extract_login_link(&l), Some(l.clone()));
        assert_eq!(extract_login_link(&format!("({l})")), Some(l.clone()));
        let web = format!("http://10.0.2.2:18300/l/{tok}");
        assert_eq!(extract_login_link(&format!("Tap {web}, then sign in")), Some(web.clone()));
        assert!(extract_login_link("see https://example.org/l/short").is_none());
        assert!(extract_login_link("https://chat.example.org").is_none());
        assert!(extract_login_link("workchat://login?server=https%3A%2F%2Fx.org").is_none(), "no token");
    }

    #[test]
    fn notification_decision() {
        let now = 1_000_000;
        let mut p = ConvPref::default_for("c");
        assert!(should_notify(None, now, "auto", false));
        assert!(!should_notify(None, now, "dnd", true));
        p.mute_until_ms = Some(-1);
        assert!(!should_notify(Some(&p), now, "auto", true));
        p.mute_until_ms = Some(now + 5);
        assert!(!should_notify(Some(&p), now, "auto", false));
        p.mute_until_ms = Some(now - 5);
        assert!(should_notify(Some(&p), now, "auto", false), "expired mute");
        p.mute_until_ms = None;
        p.notify = "mentions".into();
        assert!(!should_notify(Some(&p), now, "auto", false));
        assert!(should_notify(Some(&p), now, "auto", true));
        p.notify = "nothing".into();
        assert!(!should_notify(Some(&p), now, "auto", true));
    }

    #[test]
    fn mute_choices() {
        assert_eq!(mute_until("1h", 0, 99), Some(3_600_000));
        assert_eq!(mute_until("tomorrow", 0, 99), Some(99));
        assert_eq!(mute_until("forever", 0, 99), Some(-1));
        assert_eq!(mute_until("zzz", 0, 99), None);
    }

    #[test]
    fn at_token_parsing() {
        assert_eq!(at_tokens("hi @Bob and @design-team, @here! mail a@b.c"), vec!["bob", "design-team", "here"]);
        assert_eq!(at_tokens("@channel"), vec!["channel"]);
        assert!(at_tokens("no mentions @ here").is_empty());
        assert!(is_broadcast("everyone") && !is_broadcast("bob"));
    }
}
