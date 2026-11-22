await api.openChat("David Kim");
await api.scrollTo("button.play", -1, "end");
await api.click("button.play");
await api.wait(1000);
const a = api.app.audio;
await api.mark(`v-${a.state}-${Math.round(a.pos)}-${a.dur}-${a.error}-${a.id.slice(0,8)}`.replace(/\W+/g,"_"));
await api.wait(2000);
const b = api.app.audio;
await api.mark(`v2-${b.state}-${Math.round(b.pos)}-${b.dur}`.replace(/\W+/g,"_"));
