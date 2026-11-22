import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { hasFormatting, parse, toPlain } from "../src/lib/markup";

const cases: { name: string; input: string; blocks: unknown }[] = JSON.parse(readFileSync("tests/markup-cases.json", "utf8"));

test("shared fixtures (same file as the Rust tests)", () => {
  assert.ok(cases.length > 15);
  for (const c of cases) assert.deepEqual(parse(c.input), c.blocks, `case: ${c.name}`);
});

test("plain previews match the Rust ones", () => {
  assert.equal(toPlain("*Hello* _world_ ~x~ `code`"), "Hello world x code");
  assert.equal(toPlain("- one\n- two\n\nafter"), "one two after");
  assert.equal(toPlain("> quoted\nline"), "quoted line");
  assert.equal(toPlain("```\nlet a = 1;\n```"), "let a = 1;");
  assert.equal(toPlain(""), "");
});

test("formatting detection", () => {
  assert.equal(hasFormatting("just words"), false);
  assert.equal(hasFormatting("some *bold* words"), true);
  assert.equal(hasFormatting("- list"), true);
});
