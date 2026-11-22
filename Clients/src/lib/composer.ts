// Pure text transforms behind the composer's rich-text toolbar. The composer is a plain textarea holding the server's
// markup (see markup.ts); each toolbar button edits that text and returns the new selection.

export type FormatKind = "bold" | "italic" | "strike" | "code" | "bullet" | "numbered" | "quote" | "codeblock" | "link";

export interface Edit {
  text: string;
  start: number;
  end: number;
}

const MARK: Record<string, string> = { bold: "*", italic: "_", strike: "~", code: "`" };

/** Selection edge without surrounding whitespace: a marker next to a space is not a marker ("* word *" is plain text). */
function trimSel(text: string, start: number, end: number): [number, number] {
  while (start < end && /\s/.test(text[start])) start++;
  while (end > start && /\s/.test(text[end - 1])) end--;
  return [start, end];
}

/** Toggle with nothing selected: an empty pair is removed, the caret before a closing marker steps out of the run,
 *  inside a run the markers are removed, anywhere else a pair is inserted with the caret between. */
function caret(text: string, pos: number, m: string): Edit {
  const ls = text.lastIndexOf("\n", pos - 1) + 1;
  let le = text.indexOf("\n", pos);
  if (le < 0) le = text.length;
  const line = text.slice(ls, le);
  const at = pos - ls;
  const marks: number[] = [];
  for (let i = 0; i < line.length; i++) if (line[i] === m) marks.push(i);
  for (let k = 0; k + 1 < marks.length; k += 2) {
    const [o, c] = [marks[k], marks[k + 1]];
    if (at <= o || at > c) continue;
    if (c === o + 1) return { text: text.slice(0, ls + o) + text.slice(ls + c + 1), start: ls + o, end: ls + o }; // empty pair
    if (at === c) return { text: text, start: pos + 1, end: pos + 1 }; // step out of the run
    const out = text.slice(0, ls + o) + line.slice(o + 1, c) + text.slice(ls + c + 1);
    return { text: out, start: pos - 1, end: pos - 1 }; // unwrap
  }
  return { text: text.slice(0, pos) + m + m + text.slice(pos), start: pos + 1, end: pos + 1 };
}

function single(text: string, start0: number, end0: number, m: string): Edit {
  const [start, end] = trimSel(text, start0, end0);
  if (start >= end) return caret(text, start0, m);
  const sel = text.slice(start, end);
  // The markers sit just outside the selection: remove them.
  if (start >= m.length && text.slice(start - m.length, start) === m && text.slice(end, end + m.length) === m) {
    return { text: text.slice(0, start - 1) + sel + text.slice(end + 1), start: start - 1, end: end - 1 };
  }
  // The selection includes the markers: unwrap.
  if (sel.length >= 2 * m.length && sel.startsWith(m) && sel.endsWith(m)) {
    const inner = sel.slice(m.length, sel.length - m.length);
    return { text: text.slice(0, start) + inner + text.slice(end), start, end: start + inner.length };
  }
  return { text: text.slice(0, start) + m + sel + m + text.slice(end), start: start + 1, end: end + 1 };
}

/** A selection over several lines is formatted line by line, after any list or quote prefix (a mark never spans a line break). */
function multi(text: string, start: number, end: number, m: string): Edit {
  const lines = text.slice(start, end).split("\n");
  const split = (l: string) => {
    const pre = /^\s*(?:(?:- |• |\d{1,3}\. |> ?))?/.exec(l)![0];
    const rest = l.slice(pre.length);
    const t = rest.trim();
    return { pre, lead: rest.length - rest.trimStart().length, t, tail: rest.slice(rest.length - (rest.length - rest.trimEnd().length)) };
  };
  const wrapped = (t: string) => t.length >= 2 * m.length && t.startsWith(m) && t.endsWith(m);
  const parts = lines.map(split);
  const body = parts.filter((p) => p.t !== "");
  const all = body.length > 0 && body.every((p) => wrapped(p.t));
  const out = parts.map((p) => {
    if (p.t === "") return p.pre + " ".repeat(p.lead);
    const t = all ? p.t.slice(m.length, p.t.length - m.length) : wrapped(p.t) ? p.t : m + p.t + m;
    return p.pre + " ".repeat(p.lead) + t + p.tail;
  });
  const joined = out.join("\n");
  return { text: text.slice(0, start) + joined + text.slice(end), start, end: start + joined.length };
}

function inline(text: string, start: number, end: number, m: string): Edit {
  if (start === end) return caret(text, start, m);
  return text.slice(start, end).includes("\n") ? multi(text, start, end, m) : single(text, start, end, m);
}

const BULLET = /^(- |• )/;
const NUMBER = /^\d{1,3}\. /;
const QUOTE = /^> ?/;

function lineEdit(text: string, start: number, end: number, kind: "bullet" | "numbered" | "quote"): Edit {
  const ls = text.lastIndexOf("\n", start - 1) + 1;
  let le = text.indexOf("\n", end);
  if (le < 0) le = text.length;
  // A selection that ends right after a newline does not include the next line.
  if (end > start && text[end - 1] === "\n") le = end - 1;
  const lines = text.slice(ls, le).split("\n");
  const re = kind === "bullet" ? BULLET : kind === "numbered" ? NUMBER : QUOTE;
  const nonEmpty = lines.filter((l) => l.trim() !== "");
  const allHave = nonEmpty.length > 0 && nonEmpty.every((l) => re.test(l));
  let n = 0;
  const out = lines.map((l) => {
    if (allHave) return l.replace(re, "");
    if (l.trim() === "" && lines.length > 1) return l;
    const bare = l.replace(BULLET, "").replace(NUMBER, "").replace(kind === "quote" ? QUOTE : /^$/, "");
    if (kind === "bullet") return "- " + bare;
    if (kind === "numbered") return `${++n}. ` + bare;
    return "> " + bare;
  });
  const joined = out.join("\n");
  return { text: text.slice(0, ls) + joined + text.slice(le), start: ls, end: ls + joined.length };
}

function codeBlock(text: string, start: number, end: number): Edit {
  const sel = text.slice(start, end);
  // The selection is already a fenced block: remove the fences.
  const fenced = /^```[^\n]*\n([\s\S]*?)\n?```$/.exec(sel);
  if (fenced) return { text: text.slice(0, start) + fenced[1] + text.slice(end), start, end: start + fenced[1].length };
  const lead = start > 0 && text[start - 1] !== "\n" ? "\n" : "";
  const trail = end < text.length && text[end] !== "\n" ? "\n" : "";
  const ins = `${lead}\`\`\`\n${sel}\n\`\`\`${trail}`;
  const caret = start + lead.length + 4;
  return { text: text.slice(0, start) + ins + text.slice(end), start: caret, end: caret + sel.length };
}

function link(text: string, start: number, end: number, url: string): Edit {
  const gap = end > 0 && !/\s/.test(text[end - 1] ?? " ") ? " " : "";
  const ins = gap + url;
  const pos = end + ins.length;
  return { text: text.slice(0, end) + ins + text.slice(end), start: pos, end: pos };
}

/** Applies one toolbar action to `text` with the selection `start..end`. `url` is only used for `link`. */
export function applyFormat(text: string, start: number, end: number, kind: FormatKind, url = ""): Edit {
  if (kind in MARK) return inline(text, start, end, MARK[kind]);
  if (kind === "bullet" || kind === "numbered" || kind === "quote") return lineEdit(text, start, end, kind);
  if (kind === "codeblock") return codeBlock(text, start, end);
  return link(text, start, end, url);
}

/** Which inline formats are active at the caret/selection (for toolbar button states). */
export function activeFormats(text: string, start: number, end: number): Set<FormatKind> {
  const on = new Set<FormatKind>();
  const kinds: [FormatKind, string][] = [["bold", "*"], ["italic", "_"], ["strike", "~"], ["code", "`"]];
  for (const [k, m] of kinds) {
    if (end > start) {
      if (text.slice(start - m.length, start) === m && text.slice(end, end + m.length) === m) on.add(k);
    } else {
      // Inside a marked run on the current line?
      const ls = text.lastIndexOf("\n", start - 1) + 1;
      const before = text.slice(ls, start);
      const leQ = text.indexOf("\n", start);
      const after = text.slice(start, leQ < 0 ? text.length : leQ);
      const openIdx = before.lastIndexOf(m);
      if (openIdx >= 0 && (before.split(m).length - 1) % 2 === 1 && after.includes(m)) on.add(k);
    }
  }
  const ls = text.lastIndexOf("\n", start - 1) + 1;
  const line = text.slice(ls, text.indexOf("\n", start) < 0 ? text.length : text.indexOf("\n", start));
  if (BULLET.test(line)) on.add("bullet");
  if (NUMBER.test(line)) on.add("numbered");
  if (QUOTE.test(line)) on.add("quote");
  return on;
}

/** Keyboard shortcut to toolbar action. `code` is the physical key (so Shift+7 works on every layout). */
export function shortcutFor(e: { key: string; code?: string; ctrlKey: boolean; metaKey?: boolean; shiftKey: boolean; altKey: boolean }): FormatKind | null {
  if (!(e.ctrlKey || e.metaKey)) return null;
  const k = e.key.toLowerCase();
  if (e.altKey) return e.code === "KeyC" || k === "c" ? "codeblock" : null;
  if (e.shiftKey) {
    if (k === "x") return "strike";
    if (e.code === "Digit7") return "numbered";
    if (e.code === "Digit8") return "bullet";
    if (e.code === "Digit9") return "quote";
    return null;
  }
  if (k === "b") return "bold";
  if (k === "i") return "italic";
  if (k === "e") return "code";
  if (k === "k") return "link";
  return null;
}
