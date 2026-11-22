const FX = "/tmp/claude-1000/-run-host-home-owner/dff9a50b-92e8-45fa-a232-7d93bac18a4f/scratchpad/fx/";
const [pic] = await api.core.stagePaths([FX + "harbour-sunrise.jpg"]);
const id = Object.values(api.app.rows).find((r) => r.title === "site-visits").id;
let r = "ok";
try { await api.core.setAvatar("conv", id, pic.id); } catch (e) { r = (e.code || "") + "_" + (e.message || JSON.stringify(e)); }
await api.wait(1500);
await api.mark("setavatar-" + r.replace(/\W+/g, "_").slice(0, 90) + "-" + (api.app.rows[id]?.avatar ? "has" : "none"));
