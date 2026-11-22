// Message markup parser. Twin of src-tauri/src/markup.rs: both are tested against tests/markup-cases.json.
//
// Syntax: *bold*, _italic_, ~strike~, `code`, fenced ``` blocks, "> quote", "- bullet", "1. numbered",
// @username mentions and bare http(s):// links.

export interface Span {
  text: string;
  bold?: boolean;
  italic?: boolean;
  strike?: boolean;
  code?: boolean;
  link?: string;
  mention?: string;
}

export type Block =
  | { t: "p"; spans: Span[] }
  | { t: "code"; text: string }
  | { t: "quote"; spans: Span[] }
  | { t: "list"; ordered: boolean; items: Span[][] };

const quoteLine = (t: string): string | null => (t.startsWith("> ") ? t.slice(2) : t === ">" ? "" : null);
const bullet = (t: string): string | null => (t.startsWith("- ") ? t.slice(2) : t.startsWith("• ") ? t.slice(2) : null);
function numbered(t: string): string | null {
  const m = /^(\d{1,3})\. /.exec(t);
  return m ? t.slice(m[0].length) : null;
}
const trimStart = (s: string) => s.replace(/^\s+/, "");

/** Rust's `str::lines`: split on \n, drop a trailing \r, no empty last line. */
function lines(input: string): string[] {
  if (input === "") return [];
  const parts = input.split("\n");
  if (parts[parts.length - 1] === "") parts.pop();
  return parts.map((l) => (l.endsWith("\r") ? l.slice(0, -1) : l));
}

export function parse(input: string): Block[] {
  const blocks: Block[] = [];
  const ls = lines(input);
  let i = 0;
  let para: string[] = [];
  const flush = () => {
    if (para.length) {
      blocks.push({ t: "p", spans: parseInline(para.join("\n")) });
      para = [];
    }
  };
  while (i < ls.length) {
    const line = ls[i];
    const t = trimStart(line);
    if (t.startsWith("```")) {
      flush();
      const code: string[] = [];
      const after = t.replace(/^`+/, "");
      const end = after.indexOf("```");
      if (end >= 0) {
        blocks.push({ t: "code", text: after.slice(0, end).trim() });
        i++;
        continue;
      }
      const first = after.trim();
      if (first && !/^[\p{L}\p{N}+-]+$/u.test(first)) code.push(first);
      i++;
      while (i < ls.length) {
        if (trimStart(ls[i]).startsWith("```")) {
          i++;
          break;
        }
        code.push(ls[i]);
        i++;
      }
      blocks.push({ t: "code", text: code.join("\n") });
      continue;
    }
    const q = quoteLine(t);
    if (q !== null) {
      flush();
      const parts = [q];
      i++;
      while (i < ls.length) {
        const r = quoteLine(trimStart(ls[i]));
        if (r === null) break;
        parts.push(r);
        i++;
      }
      blocks.push({ t: "quote", spans: parseInline(parts.join("\n")) });
      continue;
    }
    const b = bullet(t);
    if (b !== null) {
      flush();
      const items = [parseInline(b)];
      i++;
      while (i < ls.length) {
        const n = bullet(trimStart(ls[i]));
        if (n === null) break;
        items.push(parseInline(n));
        i++;
      }
      blocks.push({ t: "list", ordered: false, items });
      continue;
    }
    const n = numbered(t);
    if (n !== null) {
      flush();
      const items = [parseInline(n)];
      i++;
      while (i < ls.length) {
        const m = numbered(trimStart(ls[i]));
        if (m === null) break;
        items.push(parseInline(m));
        i++;
      }
      blocks.push({ t: "list", ordered: true, items });
      continue;
    }
    if (line.trim() === "") flush();
    else para.push(line);
    i++;
  }
  flush();
  return blocks;
}

interface Style {
  bold: boolean;
  italic: boolean;
  strike: boolean;
}

const isWs = (c: string) => /\s/u.test(c);
const BEFORE = "([{>,.;:!?\"'-*_~";
const AFTER = ")]}<,.;:!?\"'-*_~";
const boundaryBefore = (prev: string | undefined) => prev === undefined || isWs(prev) || BEFORE.includes(prev);
const boundaryAfter = (next: string | undefined) => next === undefined || isWs(next) || AFTER.includes(next);

export function parseInline(s: string): Span[] {
  const out: Span[] = [];
  inline(Array.from(s), { bold: false, italic: false, strike: false }, out);
  return merge(out);
}

function mk(text: string, st: Style, extra: Partial<Span> = {}): Span {
  const sp: Span = { text };
  if (st.bold) sp.bold = true;
  if (st.italic) sp.italic = true;
  if (st.strike) sp.strike = true;
  return Object.assign(sp, extra);
}

function findClose(chars: string[], from: number, delim: string): number {
  for (let j = from; j < chars.length; j++) if (chars[j] === delim) return j;
  return -1;
}

function inline(chars: string[], style: Style, out: Span[]) {
  let i = 0;
  let buf = "";
  const pushText = () => {
    if (buf) {
      out.push(mk(buf, style));
      buf = "";
    }
  };
  while (i < chars.length) {
    const c = chars[i];
    const prev = i > 0 ? chars[i - 1] : undefined;
    if (c === "`") {
      const end = findClose(chars, i + 1, "`");
      if (end > i + 1) {
        pushText();
        out.push(mk(chars.slice(i + 1, end).join(""), style, { code: true }));
        i = end + 1;
        continue;
      }
    } else if (c === "*" || c === "_" || c === "~") {
      if (boundaryBefore(prev) && i + 1 < chars.length && !isWs(chars[i + 1])) {
        const end = findClose(chars, i + 1, c);
        if (end >= 0) {
          const after = chars[end + 1];
          const beforeClose = chars[end - 1];
          if (end > i + 1 && !isWs(beforeClose) && boundaryAfter(after)) {
            pushText();
            const st = { ...style };
            if (c === "*") st.bold = true;
            else if (c === "_") st.italic = true;
            else st.strike = true;
            inline(chars.slice(i + 1, end), st, out);
            i = end + 1;
            continue;
          }
        }
      }
    } else if (c === "@") {
      if (boundaryBefore(prev)) {
        let j = i + 1;
        while (j < chars.length && /^[A-Za-z0-9._-]$/.test(chars[j])) j++;
        const name = chars.slice(i + 1, j).join("").replace(/[.\-_]+$/, "");
        if (name) {
          pushText();
          out.push(mk(`@${name}`, style, { mention: name }));
          i += 1 + Array.from(name).length;
          continue;
        }
      }
    } else if (c === "h" && boundaryBefore(prev)) {
      const rest = chars.slice(i).join("");
      if (rest.startsWith("http://") || rest.startsWith("https://")) {
        const url = (rest.match(/^\S+/) ?? [""])[0];
        const trimmed = url.replace(/[.,);!?'"]+$/, "");
        // Rust compares byte length; URLs are ASCII in practice.
        if (new TextEncoder().encode(trimmed).length > 8) {
          pushText();
          out.push(mk(trimmed, style, { link: trimmed }));
          i += Array.from(trimmed).length;
          continue;
        }
      }
    }
    buf += c;
    i++;
  }
  pushText();
}

function merge(spans: Span[]): Span[] {
  const out: Span[] = [];
  for (const s of spans) {
    const last = out[out.length - 1];
    if (
      last &&
      !!last.bold === !!s.bold &&
      !!last.italic === !!s.italic &&
      !!last.strike === !!s.strike &&
      !!last.code === !!s.code &&
      !last.link &&
      !s.link &&
      !last.mention &&
      !s.mention
    ) {
      last.text += s.text;
      continue;
    }
    out.push(s);
  }
  return out;
}

const spansText = (spans: Span[]) => spans.map((s) => s.text).join("");

/** One-line plain text (markup removed). Same as the Rust `to_plain`. */
export function toPlain(body: string): string {
  return parse(body)
    .map((b) => (b.t === "code" ? b.text : b.t === "list" ? b.items.map(spansText).join(" ") : spansText(b.spans)))
    .join(" ")
    .split(/\s+/)
    .filter(Boolean)
    .join(" ");
}

/** True when the text contains any formatting (used to decide whether the composer shows a live preview). */
export function hasFormatting(body: string): boolean {
  return parse(body).some((b) => b.t !== "p" || b.spans.some((s) => s.bold || s.italic || s.strike || s.code || s.link || s.mention));
}
