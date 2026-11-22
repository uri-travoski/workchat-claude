// As the server owner: create a channel with a topic, a picture and members, then look at it.
const FX = "/tmp/claude-1000/-run-host-home-owner/dff9a50b-92e8-45fa-a232-7d93bac18a4f/scratchpad/fx/";
const [pic] = await api.core.stagePaths([FX + "harbour-sunrise.jpg"]);
const users = Object.values(api.app.users).filter((u) => ["katherine.wells", "david.kim", "marcus.brody"].includes(u.username)).map((u) => u.id);
const id = await api.core.createConversation("channel", "c2-channel-test", users, "Photos and notes from site visits", pic.id);
await api.wait(2500);
await api.openChat("c2-channel-test");
await api.wait(1000);
await api.mark("admin-channel-created-" + (api.app.rows[id]?.avatar ? "with-avatar" : "no-avatar"));
