//! Message markup. The server stores the body verbatim and never renders it; every client parses it.
//!
//! Syntax: `*bold*`, `_italic_`, `~strike~`, `` `code` ``, fenced ``` blocks, `> quote`, `- bullet`,
//! `1. numbered`, `@username` mentions and bare `http(s)://` links.
//!
//! The UI has a TypeScript twin (`src/lib/markup.ts`); both run the same fixtures in
//! `tests/markup-cases.json`. The Rust side is used for chat-list previews and mention extraction.

use serde::Serialize;

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Span {
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub strike: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub code: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Username when this span is an `@mention`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Block {
    #[serde(rename = "p")]
    Para { spans: Vec<Span> },
    Code { text: String },
    Quote { spans: Vec<Span> },
    List { ordered: bool, items: Vec<Vec<Span>> },
}

pub fn parse(input: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = vec![];
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;
    let mut para: Vec<&str> = vec![];
    fn flush(para: &mut Vec<&str>, blocks: &mut Vec<Block>) {
        if !para.is_empty() {
            blocks.push(Block::Para { spans: parse_inline(&para.join("\n")) });
            para.clear();
        }
    }
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim_start();
        if t.starts_with("```") {
            flush(&mut para, &mut blocks);
            let mut code: Vec<String> = vec![];
            let after = t.trim_start_matches('`');
            // Same-line closing fence: ```code```
            if let Some(end) = after.find("```") {
                blocks.push(Block::Code { text: after[..end].trim().to_string() });
                i += 1;
                continue;
            }
            let first = after.trim();
            // A language tag after the opening fence is dropped; anything else is code.
            if !first.is_empty() && !first.chars().all(|c| c.is_alphanumeric() || c == '+' || c == '-') {
                code.push(first.to_string());
            }
            i += 1;
            while i < lines.len() {
                if lines[i].trim_start().starts_with("```") {
                    i += 1;
                    break;
                }
                code.push(lines[i].to_string());
                i += 1;
            }
            blocks.push(Block::Code { text: code.join("\n") });
            continue;
        }
        if let Some(rest) = quote_line(t) {
            flush(&mut para, &mut blocks);
            let mut q = vec![rest.to_string()];
            i += 1;
            while i < lines.len() {
                match quote_line(lines[i].trim_start()) {
                    Some(r) => {
                        q.push(r.to_string());
                        i += 1;
                    }
                    None => break,
                }
            }
            blocks.push(Block::Quote { spans: parse_inline(&q.join("\n")) });
            continue;
        }
        if let Some(item) = bullet(t) {
            flush(&mut para, &mut blocks);
            let mut items = vec![parse_inline(item)];
            i += 1;
            while i < lines.len() {
                match bullet(lines[i].trim_start()) {
                    Some(it) => {
                        items.push(parse_inline(it));
                        i += 1;
                    }
                    None => break,
                }
            }
            blocks.push(Block::List { ordered: false, items });
            continue;
        }
        if let Some(item) = numbered(t) {
            flush(&mut para, &mut blocks);
            let mut items = vec![parse_inline(item)];
            i += 1;
            while i < lines.len() {
                match numbered(lines[i].trim_start()) {
                    Some(it) => {
                        items.push(parse_inline(it));
                        i += 1;
                    }
                    None => break,
                }
            }
            blocks.push(Block::List { ordered: true, items });
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut para, &mut blocks);
        } else {
            para.push(line);
        }
        i += 1;
    }
    flush(&mut para, &mut blocks);
    blocks
}

fn quote_line(t: &str) -> Option<&str> {
    t.strip_prefix("> ").or(if t == ">" { Some("") } else { None })
}

fn bullet(t: &str) -> Option<&str> {
    t.strip_prefix("- ").or_else(|| t.strip_prefix("• "))
}

fn numbered(t: &str) -> Option<&str> {
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    t[digits..].strip_prefix(". ")
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

pub fn parse_inline(s: &str) -> Vec<Span> {
    let mut out = vec![];
    inline(s, Style::default(), &mut out);
    merge(out)
}

fn is_boundary_before(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => c.is_whitespace() || "([{>,.;:!?\"'-*_~".contains(c),
    }
}

fn is_boundary_after(next: Option<char>) -> bool {
    match next {
        None => true,
        Some(c) => c.is_whitespace() || ")]}<,.;:!?\"'-*_~".contains(c),
    }
}

fn inline(s: &str, style: Style, out: &mut Vec<Span>) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut buf = String::new();
    let push_text = |buf: &mut String, out: &mut Vec<Span>| {
        if !buf.is_empty() {
            out.push(Span { text: std::mem::take(buf), bold: style.bold, italic: style.italic, strike: style.strike, ..Default::default() });
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        match c {
            '`' => {
                if let Some(end) = find_close(&chars, i + 1, '`') {
                    if end > i + 1 {
                        push_text(&mut buf, out);
                        out.push(Span { text: chars[i + 1..end].iter().collect(), code: true, bold: style.bold, italic: style.italic, strike: style.strike, ..Default::default() });
                        i = end + 1;
                        continue;
                    }
                }
            }
            '*' | '_' | '~' => {
                if is_boundary_before(prev) && i + 1 < chars.len() && !chars[i + 1].is_whitespace() {
                    if let Some(end) = find_close(&chars, i + 1, c) {
                        let after = chars.get(end + 1).copied();
                        let before_close = chars[end - 1];
                        if end > i + 1 && !before_close.is_whitespace() && is_boundary_after(after) {
                            push_text(&mut buf, out);
                            let mut st = style;
                            match c {
                                '*' => st.bold = true,
                                '_' => st.italic = true,
                                _ => st.strike = true,
                            }
                            let inner: String = chars[i + 1..end].iter().collect();
                            inline(&inner, st, out);
                            i = end + 1;
                            continue;
                        }
                    }
                }
            }
            '@' => {
                if is_boundary_before(prev) {
                    let name: String = chars[i + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || matches!(**c, '.' | '_' | '-')).collect();
                    let name_trim = name.trim_end_matches(['.', '-', '_']).to_string();
                    if !name_trim.is_empty() {
                        push_text(&mut buf, out);
                        let n = name_trim.chars().count();
                        out.push(Span { text: format!("@{name_trim}"), mention: Some(name_trim), bold: style.bold, italic: style.italic, strike: style.strike, ..Default::default() });
                        i += 1 + n;
                        continue;
                    }
                }
            }
            'h' if is_boundary_before(prev) => {
                let rest: String = chars[i..].iter().collect();
                if rest.starts_with("http://") || rest.starts_with("https://") {
                    let url: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
                    let trimmed = url.trim_end_matches(['.', ',', ')', ';', '!', '?', '\'', '"']).to_string();
                    if trimmed.len() > 8 {
                        push_text(&mut buf, out);
                        let n = trimmed.chars().count();
                        out.push(Span { text: trimmed.clone(), link: Some(trimmed), bold: style.bold, italic: style.italic, strike: style.strike, ..Default::default() });
                        i += n;
                        continue;
                    }
                }
            }
            _ => {}
        }
        buf.push(c);
        i += 1;
    }
    push_text(&mut buf, out);
}

fn find_close(chars: &[char], from: usize, delim: char) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == delim)
}

fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = vec![];
    for s in spans {
        if let Some(last) = out.last_mut() {
            if last.bold == s.bold && last.italic == s.italic && last.strike == s.strike && last.code == s.code && last.link.is_none() && s.link.is_none() && last.mention.is_none() && s.mention.is_none() {
                last.text.push_str(&s.text);
                continue;
            }
        }
        out.push(s);
    }
    out
}

fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// One-line plain text for chat-list previews and notifications: markup removed, line breaks become spaces.
pub fn to_plain(body: &str) -> String {
    let parts: Vec<String> = parse(body)
        .iter()
        .map(|b| match b {
            Block::Para { spans } | Block::Quote { spans } => spans_text(spans),
            Block::Code { text } => text.clone(),
            Block::List { items, .. } => items.iter().map(|i| spans_text(i)).collect::<Vec<_>>().join(" "),
        })
        .collect();
    parts.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Usernames mentioned with `@name` in the body, without duplicates, in order.
pub fn mention_names(body: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    fn walk(spans: &[Span], out: &mut Vec<String>) {
        for s in spans {
            if let Some(m) = &s.mention {
                if !out.contains(m) {
                    out.push(m.clone());
                }
            }
        }
    }
    for b in parse(body) {
        match b {
            Block::Para { spans } | Block::Quote { spans } => walk(&spans, &mut out),
            Block::List { items, .. } => items.iter().for_each(|i| walk(i, &mut out)),
            Block::Code { .. } => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        input: String,
        blocks: serde_json::Value,
    }

    /// The same fixtures are asserted by the TypeScript parser (`npm test`).
    #[test]
    fn shared_fixtures() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../tests/markup-cases.json")).unwrap();
        assert!(cases.len() > 15);
        for c in cases {
            let got = serde_json::to_value(parse(&c.input)).unwrap();
            assert_eq!(got, c.blocks, "case {:?}: got {}", c.name, got);
        }
    }

    #[test]
    fn plain_previews() {
        assert_eq!(to_plain("*Hello* _world_ ~x~ `code`"), "Hello world x code");
        assert_eq!(to_plain("- one\n- two\n\nafter"), "one two after");
        assert_eq!(to_plain("> quoted\nline"), "quoted line");
        assert_eq!(to_plain("```\nlet a = 1;\n```"), "let a = 1;");
        assert_eq!(to_plain("see https://example.com/x."), "see https://example.com/x.");
        assert_eq!(to_plain(""), "");
    }

    #[test]
    fn mentions() {
        assert_eq!(mention_names("hi @alice and @bob.smith, also @alice again"), vec!["alice", "bob.smith"]);
        assert!(mention_names("mail a@b.com or `@code`").is_empty());
    }
}
