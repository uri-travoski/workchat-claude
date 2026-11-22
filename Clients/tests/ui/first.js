await api.mark("list");
const rows = Object.values(api.app.rows).map(r => r.title + " [" + r.kind + "] unread=" + r.unread);
console.log(rows.join("\n"));
await api.openChat(Object.values(api.app.rows).find(r => r.kind === "dm")?.title);
await api.mark("conv");
