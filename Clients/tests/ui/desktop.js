await api.mark("desktop-chat-list");
await api.openChat("David Kim");
await api.scrollTo(".album", -1, "start");
await api.mark("desktop-conversation-media");
await api.scrollTo(".doc", -1, "center");
await api.click("button.play", -1);
await api.wait(900);
await api.click("button[aria-label='Playback speed']");
await api.scrollTo("button.play", -1, "end");
await api.wait(500);
await api.mark("desktop-conversation-docs-voice");
await api.click("button.play", -1);
// viewer
await api.scrollTo(".album", -1, "center");
await api.click(".album .cell");
await api.wait(1000);
await api.mark("desktop-viewer");
await api.click("button[aria-label='Close']");
// recording bar
await api.pointer("button.go.mic", "pointerdown");
await api.wait(2400);
await api.mark("desktop-voice-recording");
await api.pointer("button.go.mic", "pointermove", 0, -100);
await api.click("button[aria-label='Cancel recording']");
// context menu on a message of Priya
await api.openChat("Priya Patel");
await api.contextMenu(".bubble.in");
await api.mark("desktop-context-menu");
await api.clickText("Forward");
await api.wait(500);
await api.mark("desktop-forward-sheet");
await api.click("button[aria-label='Close']");
// threads and files
await api.clickText("Threads", ".pills");
await api.wait(800);
await api.mark("desktop-threads");
await api.clickText("Files", ".pills");
await api.wait(2500);
await api.mark("desktop-files-media");
await api.clickText("Documents", ".sub");
await api.wait(1800);
await api.mark("desktop-files-docs");
await api.clickText("All", ".pills");
// thread pane
await api.openChat("Elena Vance");
await api.click(".replies");
await api.wait(1500);
await api.mark("desktop-thread-pane");
// pins sheet, info
await api.openChat("launch-war-room");
await api.click(".pinbar");
await api.wait(600);
await api.mark("desktop-pins-sheet");
await api.click("button[aria-label='Close']");
// new chat
await api.click("button.fab");
await api.mark("desktop-newchat-sheet");
await api.click("button[aria-label='Close']");
// contacts and profile
await api.click("button[aria-label='Contacts']");
await api.wait(1500);
await api.click(".p .who");
await api.wait(500);
await api.mark("desktop-contacts-profile");
await api.click("button[aria-label='Close']");
// local avatar
await api.click("button[aria-label='Chats']");
await api.openChat("Priya Patel");
await api.click("section.conv button[aria-label='More']");
await api.mark("desktop-chat-menu");
await api.clickText("Set local picture");
await api.wait(1500);
await api.mark("desktop-crop-sheet");
await api.clickText("Use picture");
await api.wait(1800);
await api.mark("desktop-local-avatar");
