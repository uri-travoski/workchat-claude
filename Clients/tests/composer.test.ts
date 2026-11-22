import { test } from "node:test";
import assert from "node:assert/strict";
import { applyFormat } from "../src/lib/composer";

// applyFormat(text, selStart, selEnd, kind) -> { text, start, end }

test("bold wraps the selection and keeps it selected", () => {
  const r = applyFormat("hello world", 6, 11, "bold");
  assert.equal(r.text, "hello *world*");
  assert.deepEqual([r.start, r.end], [7, 12]);
});

test("bold again removes the markers", () => {
  const r = applyFormat("hello *world*", 7, 12, "bold");
  assert.equal(r.text, "hello world");
  assert.deepEqual([r.start, r.end], [6, 11]);
});

test("bold with no selection inserts a pair with the caret inside", () => {
  const r = applyFormat("hi ", 3, 3, "bold");
  assert.equal(r.text, "hi **");
  assert.deepEqual([r.start, r.end], [4, 4]);
});

test("italic, strike and code use their markers", () => {
  assert.equal(applyFormat("a", 0, 1, "italic").text, "_a_");
  assert.equal(applyFormat("a", 0, 1, "strike").text, "~a~");
  assert.equal(applyFormat("a", 0, 1, "code").text, "`a`");
});

test("bullet list prefixes every selected line and toggles off", () => {
  const r = applyFormat("one\ntwo", 0, 7, "bullet");
  assert.equal(r.text, "- one\n- two");
  const back = applyFormat(r.text, 0, r.text.length, "bullet");
  assert.equal(back.text, "one\ntwo");
});

test("numbered list numbers the lines", () => {
  const r = applyFormat("a\nb\nc", 0, 5, "numbered");
  assert.equal(r.text, "1. a\n2. b\n3. c");
  assert.equal(applyFormat(r.text, 0, r.text.length, "numbered").text, "a\nb\nc");
});

test("quote prefixes lines", () => {
  assert.equal(applyFormat("wise\nwords", 0, 10, "quote").text, "> wise\n> words");
});

test("code block fences the selection on their own lines", () => {
  const r = applyFormat("let a = 1;", 0, 10, "codeblock");
  assert.equal(r.text, "```\nlet a = 1;\n```");
});

test("link wraps nothing but inserts the url", () => {
  const r = applyFormat("see ", 4, 4, "link", "https://example.com");
  assert.equal(r.text, "see https://example.com");
});

test("list line only affects the line with the caret when nothing is selected", () => {
  const r = applyFormat("first\nsecond", 8, 8, "bullet");
  assert.equal(r.text, "first\n- second");
});

// ---- toolbar behaviour end to end: apply, then parse the result like a receiver would ----
import { parse } from "../src/lib/markup";
import { activeFormats, shortcutFor, type FormatKind } from "../src/lib/composer";

const spansOf = (t: string) => {
  const b = parse(t)[0];
  return b && "spans" in b ? b.spans : [];
};

test("every inline format round-trips: selection -> markers -> parsed span", () => {
  const cases: [FormatKind, string][] = [["bold", "bold"], ["italic", "italic"], ["strike", "strike"], ["code", "code"]];
  for (const [kind, flag] of cases) {
    const r = applyFormat("say hello now", 4, 9, kind);
    const sp = spansOf(r.text).find((s) => (s as Record<string, unknown>)[flag]);
    assert.equal(sp?.text, "hello", `${kind}: ${r.text}`);
  }
});

test("a selection with spaces at its edges keeps the markers off the spaces", () => {
  const r = applyFormat("say  hello  now", 3, 12, "bold");
  assert.equal(r.text, "say  *hello*  now");
  assert.equal(spansOf(r.text).find((s) => s.bold)?.text, "hello");
});

test("toggle at the cursor: pair, type, step out, and remove an empty pair", () => {
  let r = applyFormat("a ", 2, 2, "bold");
  assert.equal(r.text, "a **");
  const typed = "a *hi*";
  r = applyFormat(typed, 5, 5, "bold"); // caret before the closing marker: step out
  assert.equal(r.text, typed);
  assert.deepEqual([r.start, r.end], [6, 6]);
  r = applyFormat("a **", 3, 3, "bold"); // empty pair: remove
  assert.equal(r.text, "a ");
  r = applyFormat("a *hi there*", 6, 6, "bold"); // inside a run: unwrap
  assert.equal(r.text, "a hi there");
});

test("multi-line selection is formatted line by line and toggles back", () => {
  const src = "one\ntwo\n\nthree";
  const r = applyFormat(src, 0, src.length, "bold");
  assert.equal(r.text, "*one*\n*two*\n\n*three*");
  const blocks = parse(r.text);
  assert.ok(blocks.length >= 1);
  assert.equal(applyFormat(r.text, r.start, r.end, "bold").text, src);
});

test("multi-line selection inside list items keeps each item formatted", () => {
  const list = applyFormat("a\nb", 0, 3, "bullet");
  const bold = applyFormat(list.text, 0, list.text.length, "bold");
  // Bold goes after the list prefix, so the list stays a list for the receiver.
  assert.equal((parse(bold.text)[0] as { t: string }).t, "list");
  assert.equal(bold.text, "- *a*\n- *b*");
  assert.equal(applyFormat(bold.text, bold.start, bold.end, "bold").text, list.text);
});

test("lists and quote round-trip through the parser", () => {
  const bl = parse(applyFormat("x\ny", 0, 3, "bullet").text)[0];
  assert.deepEqual([bl.t, bl.t === "list" && bl.ordered, bl.t === "list" && bl.items.length], ["list", false, 2]);
  const nl = parse(applyFormat("x\ny", 0, 3, "numbered").text)[0];
  assert.deepEqual([nl.t, nl.t === "list" && nl.ordered], ["list", true]);
  assert.equal(parse(applyFormat("x\ny", 0, 3, "quote").text)[0].t, "quote");
  const cb = parse(applyFormat("let a = 1;", 0, 10, "codeblock").text)[0];
  assert.deepEqual(cb, { t: "code", text: "let a = 1;" });
});

test("switching list kinds replaces the prefix instead of stacking", () => {
  const b = applyFormat("one\ntwo", 0, 7, "bullet").text;
  assert.equal(applyFormat(b, 0, b.length, "numbered").text, "1. one\n2. two");
  assert.equal(applyFormat("> q", 0, 3, "bullet").text.startsWith("- "), true);
});

test("code block toggles off again", () => {
  const on = applyFormat("let a = 1;", 0, 10, "codeblock");
  assert.equal(applyFormat(on.text, 0, on.text.length, "codeblock").text, "let a = 1;");
});

test("link inserts a bare url that parses as a link", () => {
  const r = applyFormat("docs", 0, 4, "link", "https://example.com/d");
  assert.equal(r.text, "docs https://example.com/d");
  assert.equal(spansOf(r.text).find((s) => s.link)?.link, "https://example.com/d");
});

test("toolbar buttons show the active state at the caret", () => {
  assert.ok(activeFormats("a *bold* b", 5, 5).has("bold"));
  assert.ok(activeFormats("- item", 3, 3).has("bullet"));
  assert.ok(activeFormats("> q", 2, 2).has("quote"));
  assert.ok(!activeFormats("plain", 2, 2).has("bold"));
});

test("keyboard shortcuts", () => {
  const k = (o: object) => shortcutFor({ key: "", ctrlKey: true, shiftKey: false, altKey: false, ...o });
  assert.equal(k({ key: "b" }), "bold");
  assert.equal(k({ key: "I" }), "italic");
  assert.equal(k({ key: "e" }), "code");
  assert.equal(k({ key: "k" }), "link");
  assert.equal(k({ key: "X", shiftKey: true }), "strike");
  assert.equal(k({ key: "&", code: "Digit7", shiftKey: true }), "numbered");
  assert.equal(k({ key: "*", code: "Digit8", shiftKey: true }), "bullet");
  assert.equal(k({ key: "(", code: "Digit9", shiftKey: true }), "quote");
  assert.equal(k({ key: "c", code: "KeyC", altKey: true }), "codeblock");
  assert.equal(k({ key: "b", ctrlKey: false }), null);
  assert.equal(k({ key: "z" }), null);
});
