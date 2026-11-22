import { test } from "node:test";
import assert from "node:assert/strict";
import { docLine, extOf, fileTile, isAudio, isImage, isVideo, isVoice } from "../src/lib/filetype";
import { duration, fileSize } from "../src/lib/format";

test("document tiles are coloured by type", () => {
  assert.equal(fileTile("Q3-plan.pdf").label, "PDF");
  assert.equal(fileTile("Q3-plan.pdf").fg, "#c4261f");
  assert.equal(fileTile("Budget.XLSX").label, "XLSX");
  assert.equal(fileTile("Budget.XLSX").fg, "#1a7f4b");
  assert.equal(fileTile("Deck.pptx").fg, "#c2570f");
  assert.equal(fileTile("Contract.docx").fg, "#1e5bb8");
  assert.equal(fileTile("backup.zip").label, "ZIP");
  assert.equal(fileTile("noext").label, "FILE");
  assert.equal(fileTile("weird", "application/pdf").label, "PDF", "the content type backs up a missing extension");
});

test("extensions", () => {
  assert.equal(extOf("a.tar.gz"), "gz");
  assert.equal(extOf(".hidden"), "");
  assert.equal(extOf("trailing."), "");
});

test("document line shows size and pages", () => {
  assert.equal(docLine(32768, 5, (n) => `${n / 1024} KB`), "32 KB • 5 pages");
  assert.equal(docLine(10, 1, String), "10 • 1 page");
  assert.equal(docLine(10, null, String), "10");
});

test("kinds of attachment", () => {
  assert.ok(isImage("image/png") && isVideo("video/mp4") && isAudio("audio/ogg"));
  assert.ok(!isImage("video/mp4"));
  // seeded voice notes: audio with peaks, or the apps' own file names
  assert.ok(isVoice({ name: "x.m4a", content_type: "audio/mp4", meta: { peaks: [1, 2, 3] } }));
  assert.ok(isVoice({ name: "voice-20261119-014947.m4a", content_type: "audio/mp4" }));
  assert.ok(!isVoice({ name: "song.mp3", content_type: "audio/mpeg" }));
  assert.ok(!isVoice({ name: "voice-1.pdf", content_type: "application/pdf" }));
});

test("durations and sizes read naturally", () => {
  assert.equal(duration(5000), "0:05");
  assert.equal(duration(83_000), "1:23");
  assert.equal(fileSize(900), "900 B");
});
