// Plays a seeded AAC (m4a) voice note and a seeded Opus (ogg) one and reports what the player did.
const hits = await api.core.listFiles({ kind: "audio", limit: 100 });
const report = [];
for (const want of [".m4a", ".ogg"]) {
  const h = hits.find((x) => x.attachment.name.endsWith(want) && x.sender_id !== api.app.me.user.id);
  await api.core.audioPlay(h.attachment.id, 1, 0);
  await api.wait(2500);
  const a1 = { st: api.app.audio.state, pos: api.app.audio.pos, dur: api.app.audio.dur };
  await api.core.audioCtl("speed", 2);
  await api.wait(1500);
  const a2 = { st: api.app.audio.state, pos: api.app.audio.pos, spd: api.app.audio.speed };
  await api.core.audioCtl("seek", 1000);
  await api.wait(600);
  report.push(`${want}:${a1.st}@${a1.pos}of${a1.dur}_then${a2.st}@${a2.pos}x${a2.spd}_after_seek@${api.app.audio.pos}`);
  await api.core.audioCtl("stop");
}
await api.mark("audio-report-" + report.join("-").replace(/[^\w@-]+/g, "_"));
