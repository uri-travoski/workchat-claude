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
