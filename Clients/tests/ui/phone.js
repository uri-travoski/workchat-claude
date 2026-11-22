const OUT = null;
await api.mark("phone-chat-list");
// conversation with images, video, documents, voice
await api.openChat("David Kim");
await api.scrollTo(".album", -1, "start");
await api.mark("phone-conversation-media");
await api.scrollTo(".doc", -1, "center");
await api.wait(300);
await api.mark("phone-conversation-docs");
// voice playback at 1.5x
await api.click("button.play", -1);
await api.wait(900);
await api.click("button[aria-label='Playback speed']");
await api.wait(700);
await api.mark("phone-voice-playing");
await api.click("button.play", -1);
// a seeded AAC voice note from someone else, played at 1.5x
const aud = (await api.core.listFiles({ kind: "audio", limit: 100 })).find((x) => x.attachment.name.endsWith(".m4a") && x.sender_id !== api.app.me.user.id);
await api.jumpTo(aud.conv_id, aud.message_id);
await api.wait(1500);
await api.click(`[data-mid="${aud.message_id}"] button.play`);
await api.wait(800);
await api.click(`[data-mid="${aud.message_id}"] button[aria-label='Playback speed']`);
await api.wait(800);
await api.mark("phone-voice-seeded");
await api.click(`[data-mid="${aud.message_id}"] button.play`);
await api.openChat("David Kim");
// viewer
await api.scrollTo(".album", -1, "center");
await api.click(".album .cell");
await api.wait(900);
await api.mark("phone-viewer");
await api.click("button[aria-label='Next']").catch(() => {});
await api.click("button[aria-label='Close']");
// voice recording UI
await api.pointer("button.go.mic", "pointerdown");
await api.wait(2600);
await api.mark("phone-voice-recording");
await api.pointer("button.go.mic", "pointermove", 0, -100);
await api.wait(400);
await api.mark("phone-voice-locked");
await api.click("button[aria-label='Cancel recording']");
// attach sheet and preview
await api.click("button[aria-label='Attach']");
await api.mark("phone-attach-sheet");
await api.clickText("Photos");
await api.wait(1500);
await api.mark("phone-preview-sheet");
await api.clickText("Cancel", ".sheet").catch(async () => { await api.click("button[aria-label='Close']"); });
// context menu
await api.openChat("Priya Patel");
await api.contextMenu(".bubble.in");
await api.mark("phone-context-menu");
await api.clickText("Forward");
await api.wait(500);
await api.mark("phone-forward-sheet");
await api.click("button[aria-label='Close']");
// threads and files filters
await api.click("button[aria-label='Back']");
await api.clickText("Threads", ".pills");
await api.wait(600);
await api.mark("phone-threads");
await api.clickText("Files", ".pills");
await api.wait(2500);
await api.mark("phone-files-media");
await api.clickText("Documents", ".sub");
await api.wait(1800);
await api.mark("phone-files-docs");
await api.clickText("Audio", ".sub");
await api.wait(1500);
await api.mark("phone-files-audio");
await api.clickText("All", ".pills");
// thread pane
await api.openChat("Elena Vance");
await api.click(".replies");
await api.wait(1500);
await api.mark("phone-thread-pane");
await api.click("button[aria-label='Back']");
await api.click("button[aria-label='Back']");
// new chat
await api.click("button.fab");
await api.mark("phone-newchat-sheet");
await api.clickText("New channel");
await api.wait(400);
await api.mark("phone-newchannel-sheet");
await api.click("button[aria-label='Close']");
// contacts + profile
await api.clickText("Contacts", "nav");
await api.wait(1500);
await api.mark("phone-contacts");
await api.click(".p .who");
await api.wait(500);
await api.mark("phone-profile-sheet");
await api.click("button[aria-label='Close']");
// local avatar
await api.clickText("Chats", "nav");
await api.openChat("Marcus Brody");
await api.click("button[aria-label='More']");
await api.mark("phone-chat-menu");
await api.clickText("Set local picture");
await api.wait(1500);
await api.mark("phone-crop-sheet");
await api.clickText("Use picture");
await api.wait(1500);
await api.mark("phone-local-avatar");
