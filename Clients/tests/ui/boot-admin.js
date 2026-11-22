// Signs in as admin (the seeded server) when the data folder has no session yet.
if (api.app.phase === "out") {
  await api.core.loginPassword("http://192.168.8.101:18600", "admin", "t00lk1t1");
  await api.afterLogin();
}
for (let i = 0; i < 60 && (api.app.phase !== "in" || !Object.keys(api.app.rows).length); i++) await api.wait(300);
await api.wait(1500);
