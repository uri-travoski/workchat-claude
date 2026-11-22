// Finds a chat with a video, opens it in the viewer and closes it three ways; the app must stay alive.
let found = null;
for (const r of Object.values(api.app.rows)) {
  await api.openChat(r.title);
  await api.wait(400);
  const m = (api.msgs[r.id]?.items ?? []).find((x) => x.attachments.some((a) => a.content_type.startsWith("video/")));
  if (m) { found = [r, m]; break; }
}
if (!found) await api.mark("error-no-video");
await api.scrollTo(".card.media", -1, "center").catch(() => {});
const a = found[1].attachments.find((x) => x.content_type.startsWith("video/"));
api.openMedia(found[1], a);
await api.wait(1500);
await api.mark("video-open");
await api.mark("xdo:key_Escape");
await api.wait(1000);
await api.mark("video-closed-esc");
