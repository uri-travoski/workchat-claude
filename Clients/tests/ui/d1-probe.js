const rows = Object.values(api.app.rows);
const pinned = rows.filter((r) => r.favourite).map((r) => r.title);
const locked = rows.filter((r) => r.locked).length;
await api.mark("error-info pinned=" + pinned.join("|").replace(/[^A-Za-z0-9|-]/g, "_") + " saved=" + api.app.saved.length + " rows=" + rows.length);
