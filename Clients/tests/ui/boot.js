// Signs in as katherine.wells (the seeded server) when the data folder has no session yet.
if (api.app.phase === "out") {
  await api.core.loginPassword("http://192.168.8.101:8080", "katherine.wells", "t00lk1t1");
  await api.afterLogin();
}
for (let i = 0; i < 60 && (api.app.phase !== "in" || !Object.keys(api.app.rows).length); i++) await api.wait(300);
await api.wait(1500);
// Start every run without local picture overrides left over from a previous one.
for (const k of Object.keys(api.app.localAvatars)) {
  const [kind, target] = k.split(":");
  await api.core.removeLocalAvatar(kind, target).catch(() => {});
}
await api.wait(600);
