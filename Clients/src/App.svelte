<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail, start, stop } from "./lib/app.svelte";
  import { core } from "./lib/core";
  import { showPreview, stageBrowserFiles } from "./lib/attach";
  import { runTestScript } from "./lib/testdrive";
  import { startLockWatch } from "./lib/lock";
  import { reportUi, startShell } from "./lib/shell";
  import { setPlatform, syncPush } from "./lib/push";
  import { silentUpdateCheck } from "./lib/updates";
  import SheetHost from "./components/SheetHost.svelte";
  import Drawer from "./components/Drawer.svelte";
  import Rail from "./components/Rail.svelte";
  import TopBar from "./components/TopBar.svelte";
  import ChatList from "./components/ChatList.svelte";
  import Contacts from "./components/Contacts.svelte";
  import Conversation from "./components/Conversation.svelte";
  import Icon from "./components/Icon.svelte";
  import Login from "./components/Login.svelte";
  import Settings from "./components/Settings.svelte";
  import SettingsNav from "./components/settings/Nav.svelte";
  import SettingsContent from "./components/settings/Content.svelte";
  import "./components/settings/st.css";
  import PinPad from "./components/PinPad.svelte";
  import ThreadPane from "./components/ThreadPane.svelte";
  import Splitter from "./components/Splitter.svelte";
  import { LIMITS, defaults, fit, load as loadPanes, maxFor, resize, save as savePanes } from "./lib/panes";
  import { installOverlayKeys } from "./lib/overlays";

  // Resizable panes (desktop): widths are remembered on this device.
  let panes = $state(loadPanes());
  let win = $state(typeof window !== "undefined" ? window.innerWidth : 1280);
  const threadShown = $derived(!!app.thread && !!app.selected);
  const shownPanes = $derived(fit(panes, win, threadShown));
  const setPane = (pane: "list" | "thread", v: number) => (panes = resize(shownPanes, pane, v, win, threadShown));
  const resetPane = (pane: "list" | "thread") => {
    panes = { ...panes, [pane]: defaults()[pane] };
    savePanes(panes);
  };

  let dragging = $state(false);
  $effect(() => {
    void app.selected;
    if (app.phase === "in") reportUi();
  });
  const target = () => (app.thread && app.selected === app.thread.conv ? { conv: app.thread.conv, thread: app.thread.root } : app.selected ? { conv: app.selected, thread: null as string | null } : null);

  onMount(() => {
    const onResize = () => (win = window.innerWidth);
    window.addEventListener("resize", onResize);
    const offKeys = installOverlayKeys();
    let offShell: (() => void) | undefined;
    void start().then(async () => {
      setPlatform(app.info.platform);
      startLockWatch();
      offShell = await startShell();
      void silentUpdateCheck();
      void syncPush();
      void runTestScript();
    });
    // Files dropped on the window (the webview reports real paths) and images pasted from the clipboard.
    let off: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(async ({ getCurrentWebview }) => {
      off = await getCurrentWebview().onDragDropEvent(async (ev) => {
        const t = ev.payload.type;
        if (t === "enter" || t === "over") dragging = !!app.selected && app.phase === "in";
        else if (t === "leave") dragging = false;
        else if (t === "drop") {
          dragging = false;
          const dst = target();
          if (!dst || !ev.payload.paths.length) return;
          try {
            showPreview(dst.conv, dst.thread, await core.stagePaths(ev.payload.paths));
          } catch (e) {
            fail(e);
          }
        }
      });
    }).catch(() => {});
    const paste = (e: ClipboardEvent) => {
      // Some webviews (Linux, copied images) put the picture only in `items`, not in `files`.
      const dt = e.clipboardData;
      const files = dt?.files?.length
        ? Array.from(dt.files)
        : Array.from(dt?.items ?? []).filter((i) => i.kind === "file").map((i) => i.getAsFile()).filter((f): f is File => !!f);
      const dst = target();
      if (!files.length || !dst) return;
      e.preventDefault();
      void stageBrowserFiles(files, dst.conv, dst.thread);
    };
    window.addEventListener("paste", paste);
    return () => {
      window.removeEventListener("resize", onResize);
      offKeys();
      off?.();
      offShell?.();
      window.removeEventListener("paste", paste);
      stop();
    };
  });
</script>

{#if app.phase === "loading"}
  <div class="splash"><img src="/app-icon.svg" alt="" width="84" height="84" /></div>
{:else if app.phase === "out"}
  <Login />
{:else if app.phone}
  <!-- Phone: one screen at a time. Files, People and Settings open from the hamburger drawer as full screens. -->
  <div class="phone">
    {#if app.selected && app.thread}
      <ThreadPane conv={app.thread.conv} root={app.thread.root} />
    {:else if app.selected}
      <Conversation id={app.selected} />
    {:else}
      <div class="tab">
        {#if app.tab === "chats"}<ChatList />{:else if app.tab === "contacts"}<Contacts />{:else}<Settings />{/if}
      </div>
    {/if}
    {#if app.drawer && !app.selected}<Drawer />{/if}
  </div>
{:else}
  <div class="desk">
    <TopBar />
    <div class="cols">
      <Rail />
      <div class="side" style="width:{shownPanes.list}px">
        {#if app.tab === "chats"}<ChatList />{:else if app.tab === "contacts"}<Contacts />{:else}<SettingsNav />{/if}
      </div>
      <Splitter value={shownPanes.list} min={LIMITS.list.min} max={maxFor("list", shownPanes, win, threadShown)} edge="left" label="Resize the chat list" onchange={(v) => setPane("list", v)} onreset={() => resetPane("list")} ondone={() => savePanes(panes)} />
      <main class="main">
        {#if app.tab === "settings"}
          <SettingsContent />
        {:else if app.selected}
          {#key app.selected}<Conversation id={app.selected} />{/key}
        {:else}
          <div class="empty">
            <img src="/app-icon.svg" alt="" width="88" height="88" />
            <h2>WorkChat</h2>
            <p>Select a chat to start messaging.</p>
          </div>
        {/if}
      </main>
      {#if app.thread && app.selected}
        <Splitter value={shownPanes.thread} min={LIMITS.thread.min} max={maxFor("thread", shownPanes, win, true)} edge="right" label="Resize the thread pane" onchange={(v) => setPane("thread", v)} onreset={() => resetPane("thread")} ondone={() => savePanes(panes)} />
        <div class="threadcol" style="width:{shownPanes.thread}px"><ThreadPane conv={app.thread.conv} root={app.thread.root} /></div>
      {/if}
    </div>
  </div>
{/if}

{#if app.phase === "in"}<SheetHost />{/if}
{#if app.pad}<PinPad />{/if}
{#if dragging}<div class="drop"><div><b>Drop to send</b><span>Photos, videos, audio and documents</span></div></div>{/if}

{#if app.toast}
  <div class="toast" class:err={app.toast.kind === "error"} role="status">
    {#if app.toast.kind === "error"}<Icon name="alert" size={17} />{/if}{app.toast.text}
  </div>
{/if}

<style>
  .splash { height: 100%; display: flex; align-items: center; justify-content: center; background: var(--page); }
  .phone { display: flex; flex-direction: column; height: 100%; }
  .tab { flex: 1; min-height: 0; }
  .desk { display: flex; flex-direction: column; height: 100%; }
  .cols { flex: 1; min-height: 0; display: flex; }
  .side { flex: none; display: flex; flex-direction: column; min-height: 0; border-right: 1px solid var(--line); background: var(--page); box-sizing: border-box; }
  .main { flex: 1; min-width: 0; min-height: 0; background: var(--page); }
  .threadcol { flex: none; min-width: 0; border-left: 1px solid var(--line); box-sizing: border-box; }
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; color: var(--muted); }
  .empty img { border-radius: 20px; margin-bottom: 8px; opacity: 0.95; }
  .empty h2 { margin: 0; color: var(--text); }
  .empty p { margin: 0; }
  .drop { position: fixed; inset: 0; z-index: 70; background: rgba(33, 55, 91, 0.55); display: flex; align-items: center; justify-content: center; pointer-events: none; }
  .drop div { background: var(--surface); border: 2px dashed var(--blue); border-radius: 20px; padding: 28px 44px; display: flex; flex-direction: column; text-align: center; gap: 4px; }
  .drop span { color: var(--muted); }
  .toast { position: fixed; left: 50%; transform: translateX(-50%); bottom: calc(86px + var(--safe-bottom)); background: #1b2230; color: #fff; padding: 10px 16px; border-radius: 14px; font-size: 14px; max-width: min(92vw, 460px); box-shadow: var(--shadow-pop); z-index: 90; display: flex; gap: 8px; align-items: center; }
  .toast.err { background: #8c1d18; }
</style>
