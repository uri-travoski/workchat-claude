// Clicks one attachment of each document type; with a stub xdg-open on PATH (recorder) or a failing one (exit 3 -> fallback dialog).
const must = (c, m) => { if (!c) throw new Error("check failed: " + m); };
const want = ["pdf", "docx", "xlsx", "pptx", "zip", "txt"];
const found = {};
for (const r of Object.values(api.app.rows)) {
  await api.openChat(r.title);
  await api.wait(500);
  for (const m of api.msgs[r.id]?.items ?? []) for (const a of m.attachments) {
    const ext = a.name.split(".").pop().toLowerCase();
    if (want.includes(ext) && !found[ext]) found[ext] = { conv: r.title, name: a.name };
  }
}
const fails = [];
for (const [ext, f] of Object.entries(found)) {
  await api.openChat(f.conv);
  const btn = [...document.querySelectorAll("button.dmain")].find((b) => b.getAttribute("aria-label") === "Open " + f.name);
  must(btn, "doc button for " + f.name);
  btn.scrollIntoView({ block: "center" });
  await api.wait(300);
  btn.click();
  await api.wait(2500);
  const dlg = document.querySelector("[role=alertdialog]");
  if (dlg) {
    if (!fails.length) await api.mark("d1-open-fallback-dialog-" + ext);
    fails.push(ext);
    await api.mark("xdo:key_Escape");
    await api.wait(300);
    must(!document.querySelector("[role=alertdialog]"), "Esc closes only the dialog");
    must(document.querySelector("section.conv"), "chat still open");
  }
}
await api.mark("error-info found=" + Object.keys(found).join("_") + " dialog=" + fails.join("_"));
