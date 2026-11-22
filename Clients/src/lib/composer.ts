// Pure text transforms behind the composer's rich-text toolbar. The composer is a plain textarea holding the server's
// markup (see markup.ts); each toolbar button edits that text and returns the new selection.

export type FormatKind = "bold" | "italic" | "strike" | "code" | "bullet" | "numbered" | "quote" | "codeblock" | "link";

export interface Edit {
  text: string;
  start: number;
  end: number;
}

const MARK: Record<string, string> = { bold: "*", italic: "_", strike: "~", code: "`" };

function inline(text: string, start: number, end: number, m: string): Edit {
  if (start === end) {
    // Empty selection: insert a pair and put the caret between.
    return { text: text.slice(0, start) + m + m + text.slice(end), start: start + 1, end: start + 1 };
  }
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
