let r = "no-error";
try { await api.core.createConversation("channel", "should-not-exist", [], "x"); r = "created"; } catch (e) { r = (e.code || "") + "_" + (e.message || ""); }
await api.mark("katherine-create-" + r.replace(/\W+/g, "_").slice(0, 70));
