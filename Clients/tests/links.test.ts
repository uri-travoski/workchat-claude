import { test } from "node:test";
import assert from "node:assert/strict";
import { parseAppLink } from "../src/lib/links";
import { qrSvg, vcard } from "../src/lib/qr";

test("app links parse the same however the WebView splits the URL", () => {
  assert.deepEqual(
    { ...parseAppLink("workchat://chat/abc123")!, params: undefined },
    { host: "chat", path: "abc123", params: undefined },
  );
  assert.equal(parseAppLink("workchat:///chat/abc123")?.host, "chat");
  const m = parseAppLink("workchat://m/?c=1&m=2")!;
  assert.equal(m.host, "m");
  assert.equal(m.params.get("c"), "1");
  assert.equal(m.params.get("m"), "2");
  assert.equal(parseAppLink("WorkChat://Login/tok")?.host, "login");
  assert.equal(parseAppLink("https://chat.example.com/l/tok"), null);
});

test("share card: vCard and QR", () => {
  const v = vcard({ name: "Katherine Wells", title: "Head of Ops", phone: "+1 555", username: "katherine.wells", server: "https://chat.example.com" });
  assert.match(v, /^BEGIN:VCARD/);
  assert.match(v, /FN:Katherine Wells/);
  assert.match(v, /END:VCARD/);
  const svg = qrSvg(v);
  assert.match(svg, /^data:image\/svg\+xml/);
  assert.notEqual(qrSvg("a"), qrSvg("b"));
});
